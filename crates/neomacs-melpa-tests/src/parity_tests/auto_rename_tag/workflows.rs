use expect_test::expect;

use super::ParityBatchCase;

fn typing_an_opening_tag_updates_disk_and_one_undo_restores_the_document() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name "auto-rename-tag-real-file"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "landing λ.html" root))
       (document
        (concat
         "<!doctype html>\n"
         "<html>\n<body>\n"
         "<section class=\"hero\" data-label=\"λ\">Welcome Ω</section>\n"
         "</body>\n</html>\n"))
       buffer activation-before activation-after edited undo-outcome result)
  (make-directory root t)
  (with-temp-file source (insert document))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (html-mode)
        (setq activation-before
              (list
               :feature (featurep 'auto-rename-tag)
               :mode-autoload
               (and (autoloadp (symbol-function 'auto-rename-tag-mode)) t)))
        (auto-rename-tag-mode 1)
        (setq activation-after
              (list
               :feature (featurep 'auto-rename-tag)
               :mode-autoload
               (and (autoloadp (symbol-function 'auto-rename-tag-mode)) t)))
        (setq buffer-undo-list nil)
        (goto-char (point-min))
        (search-forward "section")
        (let ((old-window-buffer (window-buffer (selected-window))))
          (unwind-protect
              (progn
                (set-window-buffer (selected-window) (current-buffer))
                (execute-kbd-macro
                 (vconcat (make-list (length "section") 127) "article")))
            (set-window-buffer (selected-window) old-window-buffer)))
        (setq edited
              (list
               :activation-before activation-before
               :activation-after activation-after
               :text (neomacs-auto-rename-tag-test--text)
               :point (point)
               :mode auto-rename-tag-mode
               :before-hook
               (neomacs-auto-rename-tag-test--hook-count
                #'auto-rename-tag--before-change before-change-functions)
               :after-hook
               (neomacs-auto-rename-tag-test--hook-count
                #'auto-rename-tag--after-change after-change-functions)
               :modified (buffer-modified-p)))
        (save-buffer)
        (let ((edited-disk
               (neomacs-auto-rename-tag-test--file-text source)))
          (setq undo-outcome
                (condition-case error-data
                    (progn (undo 1) :ok)
                  (error (list (car error-data) (cdr error-data)))))
          (setq result
                (list
                 :edited edited
                 :edited-disk edited-disk
                 :undo
                 (list
                  :outcome undo-outcome
                  :text (neomacs-auto-rename-tag-test--text)
                  :point (point)
                  :modified (buffer-modified-p)
                  :disk-before-save
                  (neomacs-auto-rename-tag-test--file-text source))))
          (save-buffer)
          (setq result
                (append
                 result
                 (list
                  :disk-after-save
                  (neomacs-auto-rename-tag-test--file-text source))))))
    (neomacs-auto-rename-tag-test--cleanup-file buffer root))
  result)
"####;
    let expect = expect![[
        r#"OK (:edited (:activation-before (:feature nil :mode-autoload t) :activation-after (:feature t :mode-autoload nil) :text "<!doctype html>\n<html>\n<body>\n<article class=\"hero\" data-label=\"λ\">Welcome Ω</article>\n</body>\n</html>\n" :point 39 :mode t :before-hook 1 :after-hook 1 :modified t) :edited-disk "<!doctype html>\n<html>\n<body>\n<article class=\"hero\" data-label=\"\316\273\">Welcome \316\251</article>\n</body>\n</html>\n" :undo (:outcome :ok :text "<!doctype html>\n<html>\n<body>\n<section class=\"hero\" data-label=\"λ\">Welcome Ω</section>\n</body>\n</html>\n" :point 39 :modified t :disk-before-save "<!doctype html>\n<html>\n<body>\n<article class=\"hero\" data-label=\"\316\273\">Welcome \316\251</article>\n</body>\n</html>\n") :disk-after-save "<!doctype html>\n<html>\n<body>\n<section class=\"hero\" data-label=\"\316\273\">Welcome \316\251</section>\n</body>\n</html>\n")"#
    ]];
    ParityBatchCase::value(
        "typing_an_opening_tag_updates_disk_and_one_undo_restores_the_document",
        elisp_form,
        expect,
    )
}

fn nested_same_name_pairs_follow_depth_in_both_edit_directions() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (html-mode)
  (insert
   "<main>\n"
   "  <section id=\"outer\">\n"
   "    <section id=\"inner\"><em>deep Ω</em></section>\n"
   "  </section>\n"
   "</main>\n")
  (auto-rename-tag-mode 1)
  (neomacs-auto-rename-tag-test--replace "section" "aside" 2)
  (let ((after-inner
         (list
          :text (neomacs-auto-rename-tag-test--text)
          :point (point)
          :previous auto-rename-tag--record-prev-word)))
    (neomacs-auto-rename-tag-test--replace "section" "article" 2)
    (list
     :after-inner after-inner
     :after-outer-closing-edit
     (list
      :text (neomacs-auto-rename-tag-test--text)
      :point (point)
      :previous auto-rename-tag--record-prev-word
      :activated auto-rename-tag--pre-command-activated))))
"####;
    let expect = expect![[
        r#"OK (:after-inner (:text "<main>\n  <section id=\"outer\">\n    <aside id=\"inner\"><em>deep Ω</em></aside>\n  </section>\n</main>\n" :point 41 :previous "") :after-outer-closing-edit (:text "<main>\n  <article id=\"outer\">\n    <aside id=\"inner\"><em>deep Ω</em></aside>\n  </article>\n</main>\n" :point 88 :previous "" :activated t))"#
    ]];
    ParityBatchCase::value(
        "nested_same_name_pairs_follow_depth_in_both_edit_directions",
        elisp_form,
        expect,
    )
}

fn namespaced_multiline_xml_tag_preserves_attributes_and_renames_its_pair() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (nxml-mode)
  (insert
   "<?xml version=\"1.0\"?>\n"
   "<root xmlns:x=\"urn:x\" xmlns:ui=\"urn:ui\">\n"
   "  <x:item\n"
   "    id=\"a>b\"\n"
   "    data-label=\"λ\">body Ω</x:item>\n"
   "</root>\n")
  (auto-rename-tag-mode 1)
  (neomacs-auto-rename-tag-test--replace "x:item" "ui:panel")
  (list
   :text (neomacs-auto-rename-tag-test--text)
   :point (point)
   :major-mode major-mode
   :mode auto-rename-tag-mode
   :activated auto-rename-tag--pre-command-activated
   :before-hook
   (neomacs-auto-rename-tag-test--hook-count
    #'auto-rename-tag--before-change before-change-functions)
   :after-hook
   (neomacs-auto-rename-tag-test--hook-count
    #'auto-rename-tag--after-change after-change-functions)))
"####;
    let expect = expect![[
        r#"OK (:text "<?xml version=\"1.0\"?>\n<root xmlns:x=\"urn:x\" xmlns:ui=\"urn:ui\">\n  <ui:panel\n    id=\"a>b\"\n    data-label=\"λ\">body Ω</ui:panel>\n</root>\n" :point 75 :major-mode nxml-mode :mode t :activated t :before-hook 1 :after-hook 1)"#
    ]];
    ParityBatchCase::value(
        "namespaced_multiline_xml_tag_preserves_attributes_and_renames_its_pair",
        elisp_form,
        expect,
    )
}

fn comments_attribute_strings_and_self_closing_tags_are_not_paired() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (html-mode)
  (insert
   "<!-- <fake>comment</fake> -->\n"
   "<div data-template=\"<fake>\">templated</div>\n"
   "<asset id=\"one\"/><asset>ordinary Ω</asset>\n"
   "<fake>real</fake>\n")
  (auto-rename-tag-mode 1)
  (goto-char (point-min))
  (search-forward "<!-- <fake")
  (delete-region (- (point) 4) (point))
  (insert "note")
  (let ((after-comment
         (list
          :text (neomacs-auto-rename-tag-test--text)
          :activated auto-rename-tag--pre-command-activated)))
    (goto-char (point-min))
    (search-forward "data-template=\"<fake")
    (delete-region (- (point) 4) (point))
    (insert "note")
    (let ((after-attribute
           (list
            :text (neomacs-auto-rename-tag-test--text)
            :activated auto-rename-tag--pre-command-activated)))
      (neomacs-auto-rename-tag-test--replace "asset" "media")
      (let ((after-self-closing
             (list
              :text (neomacs-auto-rename-tag-test--text)
              :activated auto-rename-tag--pre-command-activated)))
        (neomacs-auto-rename-tag-test--replace "fake" "entry" 2)
        (list
         :after-comment after-comment
         :after-attribute after-attribute
         :after-self-closing after-self-closing
         :after-real-pair
         (list
          :text (neomacs-auto-rename-tag-test--text)
          :point (point)
          :activated auto-rename-tag--pre-command-activated))))))
"####;
    let expect = expect![[
        r#"OK (:after-comment (:text "<!-- <note>comment</fake> -->\n<div data-template=\"<fake>\">templated</div>\n<asset id=\"one\"/><asset>ordinary Ω</asset>\n<fake>real</fake>\n" :activated nil) :after-attribute (:text "<!-- <note>comment</fake> -->\n<div data-template=\"<note>\">templated</div>\n<asset id=\"one\"/><asset>ordinary Ω</asset>\n<fake>real</fake>\n" :activated nil) :after-self-closing (:text "<!-- <note>comment</fake> -->\n<div data-template=\"<note>\">templated</div>\n<media id=\"one\"/><asset>ordinary Ω</asset>\n<fake>real</fake>\n" :activated nil) :after-real-pair (:text "<!-- <note>comment</fake> -->\n<div data-template=\"<note>\">templated</div>\n<media id=\"one\"/><asset>ordinary Ω</asset>\n<entry>real</entry>\n" :point 124 :activated t))"#
    ]];
    ParityBatchCase::value(
        "comments_attribute_strings_and_self_closing_tags_are_not_paired",
        elisp_form,
        expect,
    )
}

fn command_and_minor_mode_exclusions_follow_user_customization() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((default-commands
        (mapcar
         (lambda (command)
           (cons
            command
            (neomacs-auto-rename-tag-test--run-edit
             #'html-mode
             "<div class=\"card\">one Ω</div>"
             "div" "main" 1 command)))
         '(query-replace query-replace-regexp replace-string replace-regexp)))
       (enabled-command
        (let ((auto-rename-tag-disabled-commands nil))
          (neomacs-auto-rename-tag-test--run-edit
           #'html-mode
           "<div class=\"card\">one Ω</div>"
           "div" "main" 1 'replace-string)))
       disabled-minor enabled-minor unbound-listed-mode)
  (with-temp-buffer
    (html-mode)
    (insert "<div>one</div><span>two</span>")
    (auto-rename-tag-mode 1)
    (setq-local iedit-mode t)
    (neomacs-auto-rename-tag-test--replace "div" "main")
    (setq disabled-minor
          (list
           :text (neomacs-auto-rename-tag-test--text)
           :activated auto-rename-tag--pre-command-activated))
    (setq-local iedit-mode nil)
    (neomacs-auto-rename-tag-test--replace "span" "strong")
    (setq enabled-minor
          (list
           :text (neomacs-auto-rename-tag-test--text)
           :activated auto-rename-tag--pre-command-activated)))
  (let ((auto-rename-tag-disabled-minor-modes
         '(not-a-real-auto-rename-tag-mode)))
    (setq unbound-listed-mode
          (neomacs-auto-rename-tag-test--run-edit
           #'html-mode
           "<article><custom-box>three Ω</custom-box></article>"
           "custom-box" "x-panel")))
  (list
   :default-commands default-commands
   :custom-enabled-command enabled-command
   :disabled-minor-mode disabled-minor
   :minor-mode-cleared enabled-minor
   :unbound-listed-mode unbound-listed-mode))
"####;
    let expect = expect![[
        r#"OK (:default-commands ((query-replace :text "<main class=\"card\">one Ω</div>" :point 6 :activated nil :previous "") (query-replace-regexp :text "<main class=\"card\">one Ω</div>" :point 6 :activated nil :previous "") (replace-string :text "<main class=\"card\">one Ω</div>" :point 6 :activated nil :previous "") (replace-regexp :text "<main class=\"card\">one Ω</div>" :point 6 :activated nil :previous "")) :custom-enabled-command (:text "<main class=\"card\">one Ω</main>" :point 6 :activated t :previous "") :disabled-minor-mode (:text "<main>one</div><span>two</span>" :activated nil) :minor-mode-cleared (:text "<main>one</div><strong>two</strong>" :activated t) :unbound-listed-mode (:text "<article><x-panel>three Ω</x-panel></article>" :point 18 :activated t :previous ""))"#
    ]];
    ParityBatchCase::value(
        "command_and_minor_mode_exclusions_follow_user_customization",
        elisp_form,
        expect,
    )
}

fn real_bulk_replace_command_updates_every_tag_once_without_hook_recursion() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (html-mode)
  (insert
   "<main>"
   "<div class=\"first\">Alpha Ω</div>"
   "<div class=\"second\">Beta λ</div>"
   "</main>")
  (auto-rename-tag-mode 1)
  (let ((this-command 'replace-string))
    (replace-string "div" "section" nil (point-min) (point-max)))
  (list
   :text (neomacs-auto-rename-tag-test--text)
   :point (point)
   :mark (mark t)
   :activated auto-rename-tag--pre-command-activated
   :previous auto-rename-tag--record-prev-word
   :mode auto-rename-tag-mode))
"####;
    let expect = expect![[
        r#"OK (:text "<main><section class=\"first\">Alpha Ω</section><section class=\"second\">Beta λ</section></main>" :point 86 :mark 1 :activated nil :previous "" :mode t)"#
    ]];
    ParityBatchCase::value(
        "real_bulk_replace_command_updates_every_tag_once_without_hook_recursion",
        elisp_form,
        expect,
    )
}

fn mode_hooks_are_buffer_local_idempotent_and_removed_on_disable() -> ParityBatchCase {
    let elisp_form = r####"
(let ((first (generate-new-buffer "auto-rename-tag-first"))
      (second (generate-new-buffer "auto-rename-tag-second"))
      first-enabled first-final second-off second-final result)
  (unwind-protect
      (progn
        (with-current-buffer first
          (html-mode)
          (insert "<div>A</div><span>B</span>")
          (auto-rename-tag-mode 1)
          (auto-rename-tag-mode 1)
          (setq first-enabled
                (list
                 :mode auto-rename-tag-mode
                 :before-count
                 (neomacs-auto-rename-tag-test--hook-count
                  #'auto-rename-tag--before-change before-change-functions)
                 :after-count
                 (neomacs-auto-rename-tag-test--hook-count
                  #'auto-rename-tag--after-change after-change-functions)
                 :before-local (local-variable-p 'before-change-functions)
                 :after-local (local-variable-p 'after-change-functions)
                 :lighter (copy-tree
                           (assq 'auto-rename-tag-mode minor-mode-alist))))
          (neomacs-auto-rename-tag-test--replace "div" "main")
          (auto-rename-tag-mode -1)
          (neomacs-auto-rename-tag-test--replace "span" "strong")
          (setq first-final
                (list
                 :mode auto-rename-tag-mode
                 :before-count
                 (neomacs-auto-rename-tag-test--hook-count
                  #'auto-rename-tag--before-change before-change-functions)
                 :after-count
                 (neomacs-auto-rename-tag-test--hook-count
                  #'auto-rename-tag--after-change after-change-functions)
                 :text (neomacs-auto-rename-tag-test--text))))
        (with-current-buffer second
          (html-mode)
          (insert "<p><em>C</em></p>")
          (neomacs-auto-rename-tag-test--replace "p" "aside")
          (setq second-off
                (list
                 :mode auto-rename-tag-mode
                 :text (neomacs-auto-rename-tag-test--text)))
          (auto-rename-tag-mode 1)
          (neomacs-auto-rename-tag-test--replace "em" "strong")
          (setq second-final
                (list
                 :mode auto-rename-tag-mode
                 :text (neomacs-auto-rename-tag-test--text))))
        (setq result
              (list
               :first-enabled first-enabled
               :first-after-disable first-final
               :second-while-off second-off
               :second-after-enable second-final)))
    (when (buffer-live-p first) (kill-buffer first))
    (when (buffer-live-p second) (kill-buffer second)))
  result)
"####;
    let expect = expect![[
        r#"OK (:first-enabled (:mode t :before-count 1 :after-count 1 :before-local t :after-local t :lighter (auto-rename-tag-mode " ART")) :first-after-disable (:mode nil :before-count 0 :after-count 0 :text "<main>A</main><strong>B</span>") :second-while-off (:mode nil :text "<aside><em>C</em></p>") :second-after-enable (:mode t :text "<aside><strong>C</strong></p>"))"#
    ]];
    ParityBatchCase::value(
        "mode_hooks_are_buffer_local_idempotent_and_removed_on_disable",
        elisp_form,
        expect,
    )
}

fn incremental_outer_tag_typing_preserves_embedded_script_tag_literals() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (html-mode)
  (insert
   "<div id=\"shell\">"
   "<script>const sample = \"<div>Ω</div>\";</script>"
   "<p>body λ</p>"
   "</div>")
  (auto-rename-tag-mode 1)
  (goto-char (point-min))
  (search-forward "<di")
  (insert "x")
  (list
   :text (neomacs-auto-rename-tag-test--text)
   :point (point)
   :activated auto-rename-tag--pre-command-activated
   :previous auto-rename-tag--record-prev-word
   :mode auto-rename-tag-mode))
"####;
    let expect = expect![[
        r#"OK (:text "<dixv id=\"shell\"><script>const sample = \"<div>Ω</div>\";</script><p>body λ</p></dixv>" :point 5 :activated t :previous "div" :mode t)"#
    ]];
    ParityBatchCase::value(
        "incremental_outer_tag_typing_preserves_embedded_script_tag_literals",
        elisp_form,
        expect,
    )
}

fn unmatched_and_stray_tags_degrade_to_only_the_direct_user_edit() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :unclosed
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode "<main><div>one" "div" "section")
 :mismatched
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode "<main><div>one</span></main>" "div" "section")
 :stray-closing
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode "prefix</div>suffix" "div" "section")
 :plain-text
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode "plain div text" "div" "section"))
"####;
    let expect = expect![[
        r#"OK (:unclosed (:text "<main><section>one" :point 15 :activated t :previous "") :mismatched (:text "<main><section>one</span></main>" :point 15 :activated t :previous "") :stray-closing (:text "prefix</section>suffix" :point 16 :activated t :previous "") :plain-text (:text "plain section text" :point 14 :activated nil :previous ""))"#
    ]];
    ParityBatchCase::value(
        "unmatched_and_stray_tags_degrade_to_only_the_direct_user_edit",
        elisp_form,
        expect,
    )
}

fn raw_html_case_void_and_in_progress_boundaries_remain_stable() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :mixed-case-pair
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode
  "<main><Section class=\"card\">mixed Ω</Section></main>"
  "Section" "article")
 :case-mismatched-pair
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode
  "<main><Section>mismatched λ</section></main>"
  "Section" "article")
 :slashless-void-with-explicit-close
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode
  "<main><img src=\"hero.png\"><p>fallback Ω</p></img></main>"
  "img" "picture")
 :unfinished-start-tag
 (neomacs-auto-rename-tag-test--run-edit
  #'html-mode
  "<main><custom-box data-label=\"draft Ω\""
  "custom-box" "x-panel"))
"####;
    let expect = expect![[
        r#"OK (:mixed-case-pair (:text "<main><article class=\"card\">mixed Ω</article></main>" :point 15 :activated t :previous "") :case-mismatched-pair (:text "<main><article>mismatched λ</section></main>" :point 15 :activated t :previous "") :slashless-void-with-explicit-close (:text "<main><picture src=\"hero.png\"><p>fallback Ω</p></picture></main>" :point 15 :activated t :previous "") :unfinished-start-tag (:text "<main><x-panel data-label=\"draft Ω\"" :point 15 :activated nil :previous ""))"#
    ]];
    ParityBatchCase::value(
        "raw_html_case_void_and_in_progress_boundaries_remain_stable",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        typing_an_opening_tag_updates_disk_and_one_undo_restores_the_document(),
        nested_same_name_pairs_follow_depth_in_both_edit_directions(),
        namespaced_multiline_xml_tag_preserves_attributes_and_renames_its_pair(),
        comments_attribute_strings_and_self_closing_tags_are_not_paired(),
        command_and_minor_mode_exclusions_follow_user_customization(),
        real_bulk_replace_command_updates_every_tag_once_without_hook_recursion(),
        mode_hooks_are_buffer_local_idempotent_and_removed_on_disable(),
        incremental_outer_tag_typing_preserves_embedded_script_tag_literals(),
        unmatched_and_stray_tags_degrade_to_only_the_direct_user_edit(),
        raw_html_case_void_and_in_progress_boundaries_remain_stable(),
    ]
}
