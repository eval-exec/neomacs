use expect_test::expect;

use super::ParityBatchCase;

fn html_tag_command_selects_a_template_and_opens_its_full_documentation() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (save-window-excursion
        (neomacs-company-web-test-prepare-session
         'html-mode 'company-web-html "<main>\n  <t")
        (local-set-key (kbd "C-c w") #'company-web-html)
        (execute-kbd-macro (kbd "C-c w"))
        (let* ((names (neomacs-company-web-test-plain-candidates))
               (target-index (cl-position "template" names :test #'equal)))
          (company-select-next target-index)
          (let* ((candidate (nth company-selection company-candidates))
                 (opened
                  (list :buffer (buffer-string)
                        :prefix company-prefix
                        :candidates names
                        :selection company-selection
                        :selected
                        (neomacs-company-web-test-candidate-snapshot candidate)
                        :tooltip (and (company-tooltip-visible-p) t)
                        :documentation
                        (neomacs-company-web-test-doc-snapshot candidate))))
            (company-complete-selection)
            (list :opened opened
                  :completed
                  (list :buffer (buffer-string)
                        :point (point)
                        :active (and company-candidates t)
                        :tooltip (and (company-tooltip-visible-p) t)))))))
  (when (get-buffer "*html-documentation*")
    (kill-buffer "*html-documentation*")))
"####;
    let expect = expect![[
        r####"OK (:opened (:buffer "<main>\n  <t" :prefix "t" :candidates ("table" "tbody" "td" "template" "textarea" "tfoot" "th" "thead" "time" "title" "tr" "track" "tt") :selection 3 :selected (:text "template" :annotation " -> html" :framework "html" :inline-doc nil) :tooltip t :documentation (:buffer "*html-documentation*" :mode fundamental-mode :read-only nil :text "The HTML template element <template> is a mechanism for holding client-side content that is not to be rendered when a page is loaded but may subsequently be instantiated during runtime using JavaScript. Think of a template as a content fragment that is being stored for subsequent use in the document. The parser does process the content of the <template> element during the page load to ensure that it is valid, however.\n\nContent categories:\nMetadata content, flow content, phrasing content, script-supporting element.\n\nPermitted content:\nMetadata content, flow content, any valid HTML content that is permitted to occur within the <ol>, <dl>, <figure>, <ruby>, <object>, <video>, <audio>, <table>, <colgroup>, <thead>, <tbody>, <tfoot>, <tr>, <fieldset>, <select>, <details> elements and <menu> whose type attribute is in popup menu state.\n\nTag omission:\nNone, both the starting and ending tag are mandatory.\n\nPermitted parent elements:\n<body>, <frameset>, <head> and <colgroup> without a span attribute.\n\nDOM interface:\nHTMLTemplateElement\n" :faces ((28 36 company-web-doc-tag-face "template") (347 355 company-web-doc-tag-face "template") (424 443 company-web-doc-header-1-face "Content categories:") (522 540 company-web-doc-header-1-face "Permitted content:") (635 637 company-web-doc-tag-face "ol") (641 643 company-web-doc-tag-face "dl") (647 653 company-web-doc-tag-face "figure") (657 661 company-web-doc-tag-face "ruby") (665 671 company-web-doc-tag-face "object") (675 680 company-web-doc-tag-face "video") (684 689 company-web-doc-tag-face "audio") (693 698 company-web-doc-tag-face "table") (702 710 company-web-doc-tag-face "colgroup") (714 719 company-web-doc-tag-face "thead") (723 728 company-web-doc-tag-face "tbody") (732 737 company-web-doc-tag-face "tfoot") (741 743 company-web-doc-tag-face "tr") (747 755 company-web-doc-tag-face "fieldset") (759 765 company-web-doc-tag-face "select") (769 776 company-web-doc-tag-face "details") (792 796 company-web-doc-tag-face "menu") (844 857 company-web-doc-header-1-face "Tag omission:") (913 939 company-web-doc-header-1-face "Permitted parent elements:") (941 945 company-web-doc-tag-face "body") (949 957 company-web-doc-tag-face "frameset") (961 965 company-web-doc-tag-face "head") (972 980 company-web-doc-tag-face "colgroup") (1009 1023 company-web-doc-header-1-face "DOM interface:")))) :completed (:buffer "<main>\n  <template" :point 19 :active nil :tooltip nil))"####
    ]];
    ParityBatchCase::value(
        "html_tag_command_selects_a_template_and_opens_its_full_documentation",
        elisp_form,
        expect,
    )
}

fn html_form_attribute_completion_preserves_metadata_and_documentation() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (save-window-excursion
        (neomacs-company-web-test-prepare-session
         'html-mode 'company-web-html "<form action=\"/deploy\" met")
        (company-begin-backend 'company-web-html)
        (let* ((candidate (car company-candidates))
               (opened
                (list :buffer (buffer-string)
                      :prefix company-prefix
                      :candidates (neomacs-company-web-test-plain-candidates)
                      :selected
                      (neomacs-company-web-test-candidate-snapshot candidate)
                      :documentation
                      (neomacs-company-web-test-doc-snapshot candidate))))
          (company-complete-selection)
          (list :opened opened
                :completed-buffer (buffer-string)
                :point (point)
                :active (and company-candidates t)))))
  (when (get-buffer "*html-documentation*")
    (kill-buffer "*html-documentation*")))
"####;
    let expect = expect![[
        r####"OK (:opened (:buffer "<form action=\"/deploy\" met" :prefix "met" :candidates ("method") :selected (:text "method" :annotation " -> html" :framework "html" :inline-doc nil) :documentation (:buffer "*html-documentation*" :mode fundamental-mode :read-only nil :text "method\n\nThe HTTP method that the browser uses to submit the form. Possible values are:\n  post: Corresponds to the HTTP POST method ; form data are included in the body of the form and sent to the server.\n  get: Corresponds to the HTTP GET method; form data are appended to the action attribute URI with a '?' as separator, and the resulting URI is sent to the server. Use this method when the form has no side-effects and contains only ASCII characters.\nThis value can be overridden by a formmethod attribute on a <button> or <input> element.\n" :faces ((88 95 company-web-doc-header-1-face "  post:") (205 211 company-web-doc-header-1-face "  get:") (516 522 company-web-doc-tag-face "button") (528 533 company-web-doc-tag-face "input")))) :completed-buffer "<form action=\"/deploy\" method" :point 30 :active nil)"####
    ]];
    ParityBatchCase::value(
        "html_form_attribute_completion_preserves_metadata_and_documentation",
        elisp_form,
        expect,
    )
}

fn html_direction_value_completion_selects_rtl_and_keeps_the_real_value_context() -> ParityBatchCase
{
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (save-window-excursion
        (neomacs-company-web-test-prepare-session
         'html-mode 'company-web-html "<section dir=\"\">" 2)
        (company-begin-backend 'company-web-html)
        (let* ((names (neomacs-company-web-test-plain-candidates))
               (target-index (cl-position "rtl" names :test #'equal)))
          (company-select-next target-index)
          (let ((candidate (nth company-selection company-candidates)))
            (list
             :opened
             (list :buffer (buffer-string)
                   :prefix company-prefix
                   :candidates names
                   :selection company-selection
                   :selected
                   (neomacs-company-web-test-candidate-snapshot candidate)
                   :documentation
                   (neomacs-company-web-test-doc-snapshot candidate))
             :completed
             (progn
               (company-complete-selection)
               (list :buffer (buffer-string)
                     :point (point)
                     :active (and company-candidates t))))))))
  (when (get-buffer "*html-documentation*")
    (kill-buffer "*html-documentation*")))
"####;
    let expect = expect![[
        r####"OK (:opened (:buffer "<section dir=\"\">" :prefix "" :candidates ("auto" "ltr" "rtl") :selection 2 :selected (:text "rtl" :annotation " -> html, G" :framework "html, G" :inline-doc nil) :documentation (:buffer "*html-documentation*" :mode fundamental-mode :read-only nil :text "" :faces nil)) :completed (:buffer "<section dir=\"rtl\">" :point 18 :active nil))"####
    ]];
    ParityBatchCase::value(
        "html_direction_value_completion_selects_rtl_and_keeps_the_real_value_context",
        elisp_form,
        expect,
    )
}

fn inline_css_value_completion_delegates_through_company_css_and_inserts_courier() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (save-window-excursion
    (neomacs-company-web-test-prepare-session
     'html-mode 'company-web-html "<article style=\"font-family: Co\">" 2)
    (company-begin-backend 'company-web-html)
    (let* ((names (neomacs-company-web-test-plain-candidates))
           (target-index (cl-position "Courier" names :test #'equal)))
      (company-select-next target-index)
      (let ((candidate (nth company-selection company-candidates)))
        (list
         :opened
         (list :buffer (buffer-string)
               :prefix company-prefix
               :candidates names
               :selection company-selection
               :selected
               (neomacs-company-web-test-candidate-snapshot candidate))
         :completed
         (progn
           (company-complete-selection)
           (list :buffer (buffer-string)
                 :point (point)
                 :active (and company-candidates t))))))))
"####;
    let expect = expect![[
        r####"OK (:opened (:buffer "<article style=\"font-family: Co\">" :prefix "Co" :candidates ("Courier") :selection 0 :selected (:text "Courier" :annotation " -> CSS" :framework "CSS" :inline-doc nil)) :completed (:buffer "<article style=\"font-family: Courier\">" :point 37 :active nil))"####
    ]];
    ParityBatchCase::value(
        "inline_css_value_completion_delegates_through_company_css_and_inserts_courier",
        elisp_form,
        expect,
    )
}

fn nested_html_with_url_colons_completes_the_innermost_direction_value() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (save-window-excursion
    (neomacs-company-web-test-prepare-session
     'html-mode 'company-web-html
     "<link href=\"\" rel=\"stylesheet\" hreflang=\"\"/>\n<!DOCTYPE html>\n<html><head>\n  <link href=\"\" rel=\"stylesheet\" hreflang=\"d\"/>\n  <div style=\"background: white\">\n    <template class=\"card\">\n      <a href=\"https://example.test/deploy:a\" dir=\"\">"
     2)
    (company-begin-backend 'company-web-html)
    (let* ((names (neomacs-company-web-test-plain-candidates))
           (target-index (cl-position "ltr" names :test #'equal)))
      (company-select-next target-index)
      (let ((opened
             (list :prefix company-prefix
                   :candidates names
                   :selection company-selection)))
        (company-complete-selection)
        (list :opened opened
              :completed-buffer (buffer-substring-no-properties
                                 (point-min) (point-max))
              :point (point)
              :active (and company-candidates t))))))
"####;
    let expect = expect![[
        r####"OK (:opened (:prefix "" :candidates ("auto" "ltr" "rtl") :selection 1) :completed-buffer "<link href=\"\" rel=\"stylesheet\" hreflang=\"\"/>\n<!DOCTYPE html>\n<html><head>\n  <link href=\"\" rel=\"stylesheet\" hreflang=\"d\"/>\n  <div style=\"background: white\">\n    <template class=\"card\">\n      <a href=\"https://example.test/deploy:a\" dir=\"ltr\">" :point 239 :active nil)"####
    ]];
    ParityBatchCase::value(
        "nested_html_with_url_colons_completes_the_innermost_direction_value",
        elisp_form,
        expect,
    )
}

fn pug_public_backend_completes_a_real_form_method_value() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (save-window-excursion
    (neomacs-company-web-test-prepare-session
     'pug-mode 'company-web-jade "form(method=\"\")" 2)
    (let ((backend-prefix (company-web-jade 'prefix)))
      (company-begin-backend 'company-web-jade)
      (let* ((names (neomacs-company-web-test-plain-candidates))
             (target-index (cl-position "POST" names :test #'equal)))
        (company-select-next target-index)
        (let ((opened
               (list :mode major-mode
                     :backend-prefix backend-prefix
                     :prefix company-prefix
                     :candidates names
                     :selection company-selection
                     :annotation
                     (company-call-backend
                      'annotation
                      (nth company-selection company-candidates)))))
          (company-complete-selection)
          (list :opened opened
                :buffer (buffer-string)
                :point (point)
                :active (and company-candidates t)))))))
"####;
    let expect = expect![[
        r####"OK (:opened (:mode pug-mode :backend-prefix "" :prefix "" :candidates ("GET" "POST") :selection 1 :annotation " -> html") :buffer "form(method=\"POST\")" :point 18 :active nil)"####
    ]];
    ParityBatchCase::value(
        "pug_public_backend_completes_a_real_form_method_value",
        elisp_form,
        expect,
    )
}

fn slim_public_backend_completes_a_real_direction_value() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (save-window-excursion
    (neomacs-company-web-test-prepare-session
     'slim-mode 'company-web-slim "section dir=\"a\"" 1)
    (let ((backend-prefix (company-web-slim 'prefix)))
      (company-begin-backend 'company-web-slim)
      (let* ((names (neomacs-company-web-test-plain-candidates))
             (target-index (cl-position "auto" names :test #'equal)))
        (company-select-next target-index)
        (let ((opened
               (list :mode major-mode
                     :backend-prefix backend-prefix
                     :prefix company-prefix
                     :candidates names
                     :selection company-selection
                     :annotation
                     (company-call-backend
                      'annotation
                      (nth company-selection company-candidates)))))
          (company-complete-selection)
          (list :opened opened
                :buffer (buffer-string)
                :point (point)
                :active (and company-candidates t)))))))
"####;
    let expect = expect![[
        r####"OK (:opened (:mode slim-mode :backend-prefix "a" :prefix "a" :candidates ("auto") :selection 0 :annotation " -> html, G") :buffer "section dir=\"auto\"" :point 18 :active nil)"####
    ]];
    ParityBatchCase::value(
        "slim_public_backend_completes_a_real_direction_value",
        elisp_form,
        expect,
    )
}

fn web_mode_command_completes_a_global_button_attribute() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (save-window-excursion
    (neomacs-company-web-test-prepare-session
     'web-mode 'company-web-html "<button type=\"button\" cla")
    (local-set-key (kbd "C-c w") #'company-web-html)
    (execute-kbd-macro (kbd "C-c w"))
    (let* ((names (neomacs-company-web-test-plain-candidates))
           (candidate (car company-candidates))
           (opened
            (list :mode major-mode
                  :prefix company-prefix
                  :candidates names
                  :selected
                  (neomacs-company-web-test-candidate-snapshot candidate)
                  :tooltip (and (company-tooltip-visible-p) t))))
      (company-complete-selection)
      (list :opened opened
            :buffer (buffer-string)
            :point (point)
            :active (and company-candidates t)
            :tooltip (and (company-tooltip-visible-p) t)))))
"####;
    let expect = expect![[
        r####"OK (:opened (:mode web-mode :prefix "cla" :candidates ("class") :selected (:text "class" :annotation " -> html, G" :framework "html, G" :inline-doc nil) :tooltip t) :buffer "<button type=\"button\" class" :point 28 :active nil :tooltip nil)"####
    ]];
    ParityBatchCase::value(
        "web_mode_command_completes_a_global_button_attribute",
        elisp_form,
        expect,
    )
}

fn jade_mode_command_selects_a_documented_button_type() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (save-window-excursion
        (neomacs-company-web-test-prepare-session
         'jade-mode 'company-web-jade "button(type=\"\")" 2)
        (local-set-key (kbd "C-c w") #'company-web-jade)
        (execute-kbd-macro (kbd "C-c w"))
        (let* ((names (neomacs-company-web-test-plain-candidates))
               (target-index (cl-position "submit" names :test #'equal)))
          (company-select-next target-index)
          (let* ((candidate (nth company-selection company-candidates))
                 (opened
                  (list :mode major-mode
                        :prefix company-prefix
                        :candidates names
                        :selection company-selection
                        :selected
                        (neomacs-company-web-test-candidate-snapshot candidate)
                        :documentation
                        (neomacs-company-web-test-doc-snapshot candidate))))
            (company-complete-selection)
            (list :opened opened
                  :buffer (buffer-string)
                  :point (point)
                  :active (and company-candidates t))))))
  (when (get-buffer "*html-documentation*")
    (kill-buffer "*html-documentation*")))
"####;
    let expect = expect![[
        r####"OK (:opened (:mode jade-mode :prefix "" :candidates ("button" "reset" "submit") :selection 2 :selected (:text "submit" :annotation " -> html" :framework "html" :inline-doc "Submits the form.") :documentation (:buffer "*html-documentation*" :mode fundamental-mode :read-only nil :text "Submits the form." :faces nil)) :buffer "button(type=\"submit\")" :point 20 :active nil)"####
    ]];
    ParityBatchCase::value(
        "jade_mode_command_selects_a_documented_button_type",
        elisp_form,
        expect,
    )
}

fn emmet_preview_advices_route_accept_and_abort_through_live_company_popups() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  ;; Company Web registers these legacy advices without forcing global
  ;; activation.  Activate the package-owned advice to exercise its advertised
  ;; behavior.  This case runs in its own editor process because old-style
  ;; advice mutates the function definitions globally.
  (ad-activate 'emmet-preview-accept)
  (ad-activate 'emmet-preview-abort)
  (with-temp-buffer
          (save-window-excursion
            (neomacs-company-web-test-prepare-session
             'html-mode 'company-web-html "div[dir=\"\"]" 2)
            (emmet-mode 1)
            (setq-local emmet-insert-flash-time -1)
            (local-set-key (kbd "C-c w") #'company-web-html)
            (emmet-preview (point-min) (point-max))
            (goto-char (point-max))
            (search-backward "\"\"")
            (forward-char 1)
            (execute-kbd-macro (kbd "C-c w"))
            (let* ((direction-names
                    (neomacs-company-web-test-plain-candidates))
                   (rtl-index
                    (cl-position "rtl" direction-names :test #'equal)))
              (company-select-next rtl-index)
              (let ((before-accept
                     (list
                      :buffer (buffer-substring-no-properties
                               (point-min) (point-max))
                      :prefix company-prefix
                      :candidates direction-names
                      :selection company-selection
                      :company-popup
                      (and (overlayp company-pseudo-tooltip-overlay) t)
                      :emmet-preview
                      (and (overlayp emmet-preview-input) t))))
                (call-interactively #'emmet-preview-accept)
                (let ((after-accept
                       (list
                        :buffer (buffer-substring-no-properties
                                 (point-min) (point-max))
                        :point (point)
                        :company-active (and company-candidates t)
                        :company-popup
                        (and (overlayp company-pseudo-tooltip-overlay) t)
                        :emmet-preview
                        (and (overlayp emmet-preview-input) t))))
                  ;; With no Company popup, the same public Emmet command now
                  ;; reaches Emmet and tears down the accepted preview.
                  (call-interactively #'emmet-preview-abort)
                  (erase-buffer)
                  (insert "form[method=\"\"]")
                  (emmet-preview (point-min) (point-max))
                  (goto-char (point-max))
                  (search-backward "\"\"")
                  (forward-char 1)
                  (execute-kbd-macro (kbd "C-c w"))
                  (let ((before-abort
                         (list
                          :buffer (buffer-substring-no-properties
                                   (point-min) (point-max))
                          :prefix company-prefix
                          :candidates
                          (neomacs-company-web-test-plain-candidates)
                          :company-popup
                          (and (overlayp company-pseudo-tooltip-overlay) t)
                          :emmet-preview
                          (and (overlayp emmet-preview-input) t))))
                    (call-interactively #'emmet-preview-abort)
                    (let ((after-company-abort
                           (list
                            :buffer (buffer-substring-no-properties
                                     (point-min) (point-max))
                            :point (point)
                            :company-active (and company-candidates t)
                            :company-popup
                            (and (overlayp company-pseudo-tooltip-overlay) t)
                            :emmet-preview
                            (and (overlayp emmet-preview-input) t))))
                      (call-interactively #'emmet-preview-abort)
                      (list
                       :before-accept before-accept
                       :after-accept after-accept
                       :before-abort before-abort
                       :after-company-abort after-company-abort
                       :after-emmet-abort
                       (list
                        :buffer (buffer-substring-no-properties
                                 (point-min) (point-max))
                        :company-active (and company-candidates t)
                        :company-popup
                        (and (overlayp company-pseudo-tooltip-overlay) t)
                        :emmet-preview
                        (and (overlayp emmet-preview-input) t)))))))))))
"####;
    let expect = expect![[
        r####"OK (:before-accept (:buffer "div[dir=\"\"]\n" :prefix "" :candidates ("auto" "ltr" "rtl") :selection 2 :company-popup t :emmet-preview t) :after-accept (:buffer "div[dir=\"rtl\"]\n" :point 13 :company-active nil :company-popup nil :emmet-preview t) :before-abort (:buffer "form[method=\"\"]\n" :prefix "" :candidates ("GET" "POST") :company-popup t :emmet-preview t) :after-company-abort (:buffer "form[method=\"\"]\n" :point 14 :company-active nil :company-popup nil :emmet-preview t) :after-emmet-abort (:buffer "form[method=\"\"]\n" :company-active nil :company-popup nil :emmet-preview nil))"####
    ]];
    ParityBatchCase::value(
        "emmet_preview_advices_route_accept_and_abort_through_live_company_popups",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn invalid_registered_data_source_surfaces_the_exact_public_completion_error() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (save-window-excursion
    (neomacs-company-web-test-prepare-session
     'html-mode 'company-web-html "<templ")
    (let ((web-completion-data-sources
           '(("broken-framework" .
              neomacs-company-web-test-unbound-source-directory)))
          (original-message (symbol-function 'message))
          observed-messages)
      (cl-letf (((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (let ((rendered (apply #'format-message
                                          format-string arguments)))
                     (push rendered observed-messages)
                     (apply original-message format-string arguments)))))
        (condition-case completion-error
            (company-begin-backend 'company-web-html)
          (error
           (list :error completion-error
                 :messages (nreverse observed-messages)
                 :buffer (buffer-string)
                 :point (point)
                 :active (and company-candidates t))))))))
"####;
    let expect = expect![[
        r####"OK (:error (user-error "Cannot complete at point") :messages ("Company: An error occurred in auto-begin" "Company: backend company-web-html error \"[company-html] invalid element neomacs-company-web-test-unbound-source-directory in ‘web-completion-data-sources’\" with args (candidates templ )") :buffer "<templ" :point 7 :active nil)"####
    ]];
    ParityBatchCase::value(
        "invalid_registered_data_source_surfaces_the_exact_public_completion_error",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        html_tag_command_selects_a_template_and_opens_its_full_documentation(),
        html_form_attribute_completion_preserves_metadata_and_documentation(),
        html_direction_value_completion_selects_rtl_and_keeps_the_real_value_context(),
        inline_css_value_completion_delegates_through_company_css_and_inserts_courier(),
        nested_html_with_url_colons_completes_the_innermost_direction_value(),
        pug_public_backend_completes_a_real_form_method_value(),
        slim_public_backend_completes_a_real_direction_value(),
        web_mode_command_completes_a_global_button_attribute(),
        jade_mode_command_selects_a_documented_button_type(),
        emmet_preview_advices_route_accept_and_abort_through_live_company_popups(),
        invalid_registered_data_source_surfaces_the_exact_public_completion_error(),
    ]
}
