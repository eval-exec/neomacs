use expect_test::expect;

use super::ParityBatchCase;

/// How a user turns the package on: `M-x ac-html-bootstrap+' in the HTML
/// buffer they are editing.  It makes `web-completion-data-sources'
/// buffer-local before pushing, so the source appears for that buffer only and
/// the registry every other buffer sees still holds just the stock "html"
/// entry; running it twice does not register twice; and
/// `company-web-bootstrap+' is the same command under the name company users
/// are told to call.  The registered location resolves to a directory that
/// really ships the data subdirectories a consumer reads.
fn registers_itself_as_a_buffer_local_completion_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "registers_itself_as_a_buffer_local_completion_source",
        r##"
        (list
         :before (achb-test-locations)
         :in-html-buffer
         (achb-test-in-document
          (list :returned (call-interactively 'ac-html-bootstrap+)
                :resolved (achb-test-locations)
                :buffer-local (local-variable-p 'web-completion-data-sources)
                :run-again (progn (call-interactively 'ac-html-bootstrap+)
                                  (achb-test-locations))
                :company-alias (eq (indirect-function 'company-web-bootstrap+)
                                   (indirect-function 'ac-html-bootstrap+))
                :ships (achb-test-shipped "Bootstrap")
                :tags (length (achb-test-lines "Bootstrap" "html-tag-list"))))
         :other-buffer
         (with-temp-buffer
           (list :resolved (achb-test-locations)
                 :buffer-local (local-variable-p 'web-completion-data-sources))))
    "##,
        expect![[
            r#"OK (:before (("html" "web-completion-data-20160318.848/html-stuff" t)) :in-html-buffer (:returned (("Bootstrap" . ac-html-bootstrap-source-dir) ("html" . web-completion-data-html-source-dir)) :resolved (("Bootstrap" "ac-html-bootstrap-20160302.1701/html-stuff" t) ("html" "web-completion-data-20160318.848/html-stuff" t)) :buffer-local t :run-again (("Bootstrap" "ac-html-bootstrap-20160302.1701/html-stuff" t) ("html" "web-completion-data-20160318.848/html-stuff" t)) :company-alias t :ships ("html-attributes-complete" "html-attributes-list" "html-attributes-short-docs" "html-tag-short-docs") :tags 19) :other-buffer (:resolved (("html" "web-completion-data-20160318.848/html-stuff" t)) :buffer-local nil))"#
        ]],
    )
}

fn offers_bootstrap_button_classes_with_their_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "offers_bootstrap_button_classes_with_their_documentation",
        r##"
        (achb-test-in-document
         (call-interactively 'ac-html-bootstrap+)
         (achb-test-goto "<button class=\"btn btn-")
         (list :point (point)
               :line (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position))
               :offer (achb-test-offer "Bootstrap")))
    "##,
        expect![[
            r#"OK (:point 231 :line "      <button class=\"btn btn-\" data-toggle=\"\">Save</button>" :offer (:context ("button" "class" "btn-") :offered 247 :matching 11 :candidates (("btn-block" . "Create block level buttons—those that span the full width of a parent") ("btn-danger") ("btn-default") ("btn-info") ("btn-lg" . "Button sizing") ("btn-link" . "Deemphasize a button by making it look like a link while maintaining button behavior") ("btn-primary") ("btn-sm" . "Button sizing") ("btn-success") ("btn-warning") ("btn-xs" . "Button sizing"))))"#
        ]],
    )
}

fn offers_a_different_class_set_for_each_tag() -> ParityBatchCase {
    ParityBatchCase::value(
        "offers_a_different_class_set_for_each_tag",
        r##"
        (achb-test-in-document
         (call-interactively 'ac-html-bootstrap+)
         (cl-flet ((offer-at (marker)
                     (achb-test-goto marker)
                     (achb-test-offer "Bootstrap")))
           (list
            :div (offer-at "<div class=\"panel-")
            :span (offer-at "<span class=\"label-")
            :td (offer-at "<td class=\"dan")
            :per-tag-isolation
            (let ((button (mapcar #'car (achb-test-values "Bootstrap" "button" "class")))
                  (div (mapcar #'car (achb-test-values "Bootstrap" "div" "class"))))
              (list :panel-body-is-a-div-class
                    (and (member "panel-body" div) t)
                    :panel-body-offered-for-button
                    (and (member "panel-body" button) t)
                    :global-class-offered-for-both
                    (list (and (member "text-center" div) t)
                          (and (member "text-center" button) t)))))))
    "##,
        expect![[
            r#"OK (:div (:context ("div" "class" "panel-") :offered 339 :matching 11 :candidates (("panel-body") ("panel-collapse" . "Be sure to add the class collapse to the collapsible element. If you'd like it to default open, add the additional class in.") ("panel-danger") ("panel-default") ("panel-footer" . "Wrap buttons or secondary text in .panel-footer.\\nNote that panel footers do not inherit colors and borders when using contextual variations as they are not meant to be in the foreground.") ("panel-group" . "<div class=\"panel-group\" id=\"accordion\" role=\"tablist\" aria-multiselectable=\"true\">\\n <div class=\"panel panel-default\">\\n   <div class=\"panel-heading\" role=\"tab\" id=\"headingOne\">") ("panel-heading" . "Container for heading and h1-h6 .panel-title") ("panel-info") ("panel-primary" . "Opposite to .panel-default") ("panel-success") ("panel-warning"))) :span (:context ("span" "class" "label-") :offered 269 :matching 6 :candidates (("label-danger") ("label-default") ("label-info") ("label-primary") ("label-success") ("label-warning"))) :td (:context ("td" "class" "dan") :offered 229 :matching 1 :candidates (("danger"))) :per-tag-isolation (:panel-body-is-a-div-class t :panel-body-offered-for-button nil :global-class-offered-for-both (t t)))"#
        ]],
    )
}

fn offers_data_attribute_values_and_their_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "offers_data_attribute_values_and_their_documentation",
        r##"
        (achb-test-in-document
         (call-interactively 'ac-html-bootstrap+)
         (achb-test-goto "data-toggle=\"")
         (list
          :values (achb-test-offer "Bootstrap")
          :attributes (mapcar #'car (achb-test-attributes "Bootstrap" "button"))
          :attribute-docs
          (list :own (achb-test-attribute-doc "Bootstrap" "div" "data-ride")
                :global-fallback
                (achb-test-attribute-doc "Bootstrap" "div" "data-container")
                :undocumented
                (achb-test-attribute-doc "Bootstrap" "button" "data-toggle"))))
    "##,
        expect![[
            r##"OK (:values (:context ("button" "data-toggle" "") :offered 7 :matching 7 :candidates (("button" . "Single toggle button\\nPre-toggled buttons need .active and aria-pressed=\"true\"\\n<button type=\"button\" class=\"btn btn-primary\" data-toggle=\"button\"\\n        aria-pressed=\"false\" autocomplete=\"off\">\\n  Single toggle\\n</button>") ("buttons" . "Togglable Checkbox or radio buttons.\\n<div class=\"btn-group\" data-toggle=\"buttons\">\\n  <label class=\"btn btn-primary active\">\\n    <input type=\"checkbox\" autocomplete=\"off\" checked> Checkbox 1 (pre-checked)\\n  </label>\\n  <label class=\"btn btn-primary\">\\n    <input type=\"checkbox\" autocomplete=\"off\"> Checkbox 2\\n  </label>\\n  <label class=\"btn btn-primary\">\\n    <input type=\"checkbox\" autocomplete=\"off\"> Checkbox 3\\n </label>\\n</div>") ("collapse") ("dropdown" . "<button id=\"dLabel\" type=\"button\" data-toggle=\"dropdown\" aria-haspopup=\"true\" role=\"button\" aria-expanded=\"false\">") ("modal" . "Activate a modal without writing JavaScript. Set data-toggle=\"modal\" on a controller element, like a button, along with a data-target=\"#foo\" or href=\"#foo\" to target a specific modal to toggle.\\n\\n<button type=\"button\" data-toggle=\"modal\" data-target=\"#myModal\">Launch modal</button>") ("popover" . "Popover.\\nFor performance reasons, the Tooltip and Popover data-apis are opt-in,\\nmeaning YOU MUST INITIALIZE THEM YOURSELF.\\n\\n$(function () {\\n  $('[data-toggle=\"popover\"]').popover({placement: 'bottom'})\\n})") ("tooltip" . "Tooltip.\\nFor performance reasons, the Tooltip and Popover data-apis are opt-in,\\nmeaning YOU MUST INITIALIZE THEM YOURSELF.\\n\\n$(function () {\\n  $('[data-toggle=\"tooltip\"]').tooltip({placement: 'bottom'})\\n})"))) :attributes ("data-dismiss" "data-loading-text" "data-slide" "data-target" "data-toggle" "data-animation" "data-container" "data-content" "data-delay" "data-html" "data-offset" "data-placement" "data-selector" "data-spy" "data-target" "data-template" "data-title" "data-trigger" "data-viewport") :attribute-docs (:own "Used to mark a carousel as animating starting at page load." :global-fallback "Appends the tooltip/popover to a specific element." :undocumented nil))"##
        ]],
    )
}

fn font_awesome_adds_icon_classes_for_i_and_nothing_else() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_awesome_adds_icon_classes_for_i_and_nothing_else",
        r##"
        (achb-test-in-document
         (call-interactively 'ac-html-bootstrap+)
         (call-interactively 'ac-html-fa+)
         (achb-test-goto "<i class=\"fa-sp")
         (list
          :sources (mapcar #'car (achb-test-sources))
          :fa-alias (eq (indirect-function 'company-web-fa+)
                        (indirect-function 'ac-html-fa+))
          :font-awesome-at-point (achb-test-offer "Font Aws")
          :bootstrap-at-point
          (let ((offer (achb-test-offer "Bootstrap")))
            (list :offered (plist-get offer :offered)
                  :matching (plist-get offer :matching)))
          :both-answer-for-i
          (list :bootstrap (length (achb-test-values "Bootstrap" "i" "class"))
                :font-awesome (length (achb-test-values "Font Aws" "i" "class"))
                :bootstrap-icons
                (seq-take (mapcar #'car (achb-test-values "Bootstrap" "i" "class")) 3))
          :font-awesome-contributes-nothing-else
          (list :ships (achb-test-shipped "Font Aws")
                :tags (achb-test-lines "Font Aws" "html-tag-list")
                :div-attributes (achb-test-attributes "Font Aws" "div")
                :div-classes (achb-test-values "Font Aws" "div" "class")
                :button-classes (achb-test-values "Font Aws" "button" "class"))))
    "##,
        expect![[
            r#"OK (:sources ("Font Aws" "Bootstrap" "html") :fa-alias t :font-awesome-at-point (:context ("i" "class" "fa-sp") :offered 616 :matching 5 :candidates (("fa-space-shuttle") ("fa-spin") ("fa-spinner") ("fa-spoon") ("fa-spotify"))) :bootstrap-at-point (:offered 425 :matching 0) :both-answer-for-i (:bootstrap 425 :font-awesome 616 :bootstrap-icons ("glyphicon" "glyphicon-adjust" "glyphicon-align-center")) :font-awesome-contributes-nothing-else (:ships ("html-attributes-complete") :tags nil :div-attributes nil :div-classes nil :button-classes nil))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        registers_itself_as_a_buffer_local_completion_source(),
        offers_bootstrap_button_classes_with_their_documentation(),
        offers_a_different_class_set_for_each_tag(),
        offers_data_attribute_values_and_their_documentation(),
        font_awesome_adds_icon_classes_for_i_and_nothing_else(),
    ]
}
