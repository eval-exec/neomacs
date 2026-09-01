use expect_test::expect;

use super::ParityBatchCase;

/// Turning Angular completion on: `M-x ac-html-angular+' in the template you
/// are editing puts the package's data source in front of the default html one
/// for that buffer alone.  The next buffer is untouched, the global value is
/// untouched, calling it twice does not duplicate the entry, and
/// `company-web-angular+' -- the name company-web users are told to call -- is
/// the very same command.  The registered entry has to resolve to a directory
/// that really ships the four completion data shapes.
fn enabling_angular_completion_prepends_its_source_for_this_buffer_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_angular_completion_prepends_its_source_for_this_buffer_only",
        r##"(list
 :ac-html-angular+
 (ac-html-angular-test-with-template
  (let ((before (list web-completion-data-sources
                      (local-variable-p 'web-completion-data-sources))))
    (ac-html-angular+)
    (let ((after web-completion-data-sources))
      (ac-html-angular+)
      (list :before before
            :after after
            :buffer-local (local-variable-p 'web-completion-data-sources)
            :twice web-completion-data-sources
            :names (mapcar #'car (ac-html-angular-test-sources))
            :global (default-value 'web-completion-data-sources)
            :directory-exists (file-directory-p
                               (ac-html-angular-test-directory "Angular15"))
            :ships (ac-html-angular-test-shipped-directories "Angular15")
            :tag-list (file-exists-p
                       (expand-file-name
                        "html-tag-list"
                        (ac-html-angular-test-directory "Angular15")))))))
 :next-buffer
 (ac-html-angular-test-with-template
  (list web-completion-data-sources
        (local-variable-p 'web-completion-data-sources)))
 :company-web-alias
 (list (eq (symbol-function 'company-web-angular+) 'ac-html-angular+)
       (ac-html-angular-test-with-template
        (company-web-angular+)
        (mapcar #'car (ac-html-angular-test-sources)))))"##,
        expect![[
            r#"OK (:ac-html-angular+ (:before (#1=(("html" . web-completion-data-html-source-dir)) nil) :after #2=(("Angular15" . ac-html-angular-source-dir) . #1#) :buffer-local t :twice #2# :names ("Angular15" "html") :global #1# :directory-exists t :ships ("html-attributes-list" "html-attributes-short-docs" "html-tag-short-docs") :tag-list t) :next-buffer (#1# nil) :company-web-alias (t ("Angular15" "html")))"#
        ]],
    )
}

fn an_angular_template_offers_the_directives_of_the_tag_being_edited() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_angular_template_offers_the_directives_of_the_tag_being_edited",
        r##"(ac-html-angular-test-with-template
 (ac-html-angular+)
 (list :input (ac-html-angular-test-candidates
               "Angular15" "html-attributes-list/input")
       :select (ac-html-angular-test-candidates
                "Angular15" "html-attributes-list/select")
       :textarea (ac-html-angular-test-candidates
                  "Angular15" "html-attributes-list/textarea")
       :ng-include (ac-html-angular-test-candidates
                    "Angular15" "html-attributes-list/ng-include")
       :tags-with-attributes (sort (mapcar #'file-name-nondirectory
                                           (directory-files
                                            (expand-file-name
                                             "html-attributes-list"
                                             (ac-html-angular-test-directory "Angular15"))
                                            nil "\\`[^.]"))
                                   #'string<)
       :merged-order (mapcar #'car (ac-html-angular-test-sources))
       :input-counts (list (length (ac-html-angular-test-attributes "Angular15" "input"))
                           (length (ac-html-angular-test-attributes "html" "input")))
       :html-input-sample (seq-take (ac-html-angular-test-attributes "html" "input") 6)))"##,
        expect![[
            r#"OK (:input ("max" "min" "name" "ng-blur" "ng-change" "ng-checked" "ng-copy" "ng-cut" "ng-disabled" "ng-false-value" "ng-focus" "ng-list" "ng-max" "ng-maxlength" "ng-min" "ng-minlength" "ng-model" "ng-paste" "ng-pattern" "ng-readonly" "ng-required" "ng-true-value" "ng-value" "pattern" "required" "value") :select ("multiple" "name" "ng-blur" "ng-change" "ng-copy" "ng-cut" "ng-focus" "ng-model" "ng-options" "ng-paste" "ng-required" "required") :textarea ("name" "ng-blur" "ng-change" "ng-copy" "ng-cut" "ng-focus" "ng-maxlength" "ng-minlength" "ng-model" "ng-paste" "ng-pattern" "ng-required" "required") :ng-include ("autoscroll" "onload") :tags-with-attributes ("a" "details" "form" "global" "html" "img" "input" "ng-include" "ng-messages" "ng-pluralize" "ng-view" "option" "script" "select" "textarea" "window") :merged-order ("Angular15" "html") :input-counts (69 92) :html-input-sample ("accept" "autocapitalize" "autocomplete" "autocorrect" "autofocus" "autosave"))"#
        ]],
    )
}

fn the_global_directives_are_offered_on_every_tag() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_global_directives_are_offered_on_every_tag",
        r##"(ac-html-angular-test-with-template
 (ac-html-angular+)
 (let ((global (ac-html-angular-test-candidates
                "Angular15" "html-attributes-list/global")))
   (list :global global
         :count (length global)
         :on-input (last (ac-html-angular-test-attributes "Angular15" "input")
                         (length global))
         :on-unknown-tag (ac-html-angular-test-attributes "Angular15" "div")
         :ng-include-attributes (ac-html-angular-test-attributes
                                 "Angular15" "ng-include"))))"##,
        expect![[
            r#"OK (:global ("ng-animate-swap" "ng-app" "ng-bind" "ng-bind-html" "ng-bind-template" "ng-class" "ng-class-even" "ng-class-odd" "ng-click" "ng-cloak" "ng-controller" "ng-dblclick" "ng-form" "ng-hide" "ng-if" "ng-include" "ng-init" "ng-jq" "ng-keydown" "ng-keypress" "ng-keyup" "ng-message" "ng-message-exp" "ng-messages" "ng-messages-include" "ng-model-options" "ng-mousedown" "ng-mouseenter" "ng-mouseleave" "ng-mousemove" "ng-mouseover" "ng-mouseup" "ng-non-bindable" "ng-options" "ng-pluralize" "ng-repeat" "ng-show" "ng-style" "ng-swipe-left" "ng-swipe-right" "ng-switch" "ng-transclude" "ng-view") :count 43 :on-input ("ng-animate-swap" "ng-app" "ng-bind" "ng-bind-html" "ng-bind-template" "ng-class" "ng-class-even" "ng-class-odd" "ng-click" "ng-cloak" "ng-controller" "ng-dblclick" "ng-form" "ng-hide" "ng-if" "ng-include" "ng-init" "ng-jq" "ng-keydown" "ng-keypress" "ng-keyup" "ng-message" "ng-message-exp" "ng-messages" "ng-messages-include" "ng-model-options" "ng-mousedown" "ng-mouseenter" "ng-mouseleave" "ng-mousemove" "ng-mouseover" "ng-mouseup" "ng-non-bindable" "ng-options" "ng-pluralize" "ng-repeat" "ng-show" "ng-style" "ng-swipe-left" "ng-swipe-right" "ng-switch" "ng-transclude" "ng-view") :on-unknown-tag ("ng-animate-swap" "ng-app" "ng-bind" "ng-bind-html" "ng-bind-template" "ng-class" "ng-class-even" "ng-class-odd" "ng-click" "ng-cloak" "ng-controller" "ng-dblclick" "ng-form" "ng-hide" "ng-if" "ng-include" "ng-init" "ng-jq" "ng-keydown" "ng-keypress" "ng-keyup" "ng-message" "ng-message-exp" "ng-messages" "ng-messages-include" "ng-model-options" "ng-mousedown" "ng-mouseenter" "ng-mouseleave" "ng-mousemove" "ng-mouseover" "ng-mouseup" "ng-non-bindable" "ng-options" "ng-pluralize" "ng-repeat" "ng-show" "ng-style" "ng-swipe-left" "ng-swipe-right" "ng-switch" "ng-transclude" "ng-view") :ng-include-attributes ("autoscroll" "onload" "ng-animate-swap" "ng-app" "ng-bind" "ng-bind-html" "ng-bind-template" "ng-class" "ng-class-even" "ng-class-odd" "ng-click" "ng-cloak" "ng-controller" "ng-dblclick" "ng-form" "ng-hide" "ng-if" "ng-include" "ng-init" "ng-jq" "ng-keydown" "ng-keypress" "ng-keyup" "ng-message" "ng-message-exp" "ng-messages" "ng-messages-include" "ng-model-options" "ng-mousedown" "ng-mouseenter" "ng-mouseleave" "ng-mousemove" "ng-mouseover" "ng-mouseup" "ng-non-bindable" "ng-options" "ng-pluralize" "ng-repeat" "ng-show" "ng-style" "ng-swipe-left" "ng-swipe-right" "ng-switch" "ng-transclude" "ng-view"))"#
        ]],
    )
}

fn every_offered_directive_carries_the_angular_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_offered_directive_carries_the_angular_documentation",
        r##"(ac-html-angular-test-with-template
 (ac-html-angular+)
 (list :tag-specific (ac-html-angular-test-attribute-doc
                      "Angular15" "input" "ng-model")
       :select-specific (ac-html-angular-test-attribute-doc
                         "Angular15" "select" "ng-options")
       :global-fallback (ac-html-angular-test-attribute-doc
                         "Angular15" "input" "ng-click")
       :markdown (seq-take
                  (split-string
                   (ac-html-angular-test-attribute-doc "Angular15" "global" "ng-repeat")
                   "\n")
                  6)
       :not-shipped (ac-html-angular-test-attribute-doc
                     "Angular15" "input" "onclick")
       :undocumented (ac-html-angular-test-undocumented "Angular15")))"##,
        expect![[
            r#"OK (:tag-specific "Assignable angular expression to data-bind to." :select-specific "sets the options that the select is populated with and defines what is\nset on the model on selection. See `ngOptions`." :global-fallback "The ngClick directive allows you to specify custom behavior when\nan element is clicked." :markdown ("The `ngRepeat` directive instantiates a template once per item from a collection. Each template" "instance gets its own scope, where the given loop variable is set to the current collection item," "and `$index` is set to the item index or key." "Special properties are exposed on the local scope of each template instance, including:" "| Variable  | Type            | Details                                                                     |" "|-----------|-----------------|-----------------------------------------------------------------------------|") :not-shipped nil :undocumented nil)"#
        ]],
    )
}

fn angular_elements_are_offered_as_tags_with_their_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "angular_elements_are_offered_as_tags_with_their_documentation",
        r##"(ac-html-angular-test-with-template
 (ac-html-angular+)
 (let* ((directory (ac-html-angular-test-directory "Angular15"))
        (tags (ac-html-angular-test-candidates "Angular15" "html-tag-list")))
   (list :tags tags
         :count (length tags)
         :ng-view-doc (seq-take
                       (split-string (ac-html-angular-test-tag-doc "Angular15" "ng-view") "\n")
                       3)
         :ng-include-doc (car (split-string
                               (ac-html-angular-test-tag-doc "Angular15" "ng-include") "\n"))
         :undocumented-tags (cl-remove-if
                             (lambda (tag) (ac-html-angular-test-tag-doc "Angular15" tag))
                             tags)
         :attrv-list (file-exists-p (expand-file-name "html-attrv-list" directory))
         :attrv-docs (file-exists-p (expand-file-name "html-attrv-docs" directory))
         :html-tags-count (length (ac-html-angular-test-candidates
                                   "html" "html-tag-list")))))"##,
        expect![[
            r##"OK (:tags ("a" "form" "input" "ng-form" "ng-include" "ng-message" "ng-message-exp" "ng-messages" "ng-messages-include" "ng-pluralize" "ng-switch" "ng-transclude" "ng-view" "script" "select" "textarea") :count 16 :ng-view-doc ("# Overview" "`ngView` is a directive that complements the $route service by" "including the rendered template of the current route into the main layout (`index.html`) file.") :ng-include-doc "Fetches, compiles and includes an external HTML fragment." :undocumented-tags nil :attrv-list nil :attrv-docs nil :html-tags-count 147)"##
        ]],
    )
}

fn a_plain_html_buffer_never_offers_the_angular_directives() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_plain_html_buffer_never_offers_the_angular_directives",
        r##"(ac-html-angular-test-with-template
 (let* ((plain-sources (mapcar #'car (ac-html-angular-test-sources)))
        (plain-input (ac-html-angular-test-attributes "html" "input"))
        (plain-angular (ac-html-angular-test-attributes "Angular15" "input"))
        (plain-tags (member "ng-view" (ac-html-angular-test-candidates
                                       "html" "html-tag-list"))))
   (ac-html-angular+)
   (let ((angular-input (ac-html-angular-test-attributes "Angular15" "input")))
     (list :plain-sources plain-sources
           :plain-angular-attributes plain-angular
           :ng-view-is-not-html-tag plain-tags
           :plain-input-has-no-directives
           (cl-remove-if-not (lambda (attribute) (string-prefix-p "ng-" attribute))
                             plain-input)
           :enabled-sources (mapcar #'car (ac-html-angular-test-sources))
           :gained (length angular-input)
           :gained-directives (length
                               (cl-remove-if-not
                                (lambda (attribute) (string-prefix-p "ng-" attribute))
                                angular-input))))))"##,
        expect![[
            r#"OK (:plain-sources ("html") :plain-angular-attributes nil :ng-view-is-not-html-tag nil :plain-input-has-no-directives nil :enabled-sources ("Angular15" "html") :gained 69 :gained-directives 63)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_angular_completion_prepends_its_source_for_this_buffer_only(),
        an_angular_template_offers_the_directives_of_the_tag_being_edited(),
        the_global_directives_are_offered_on_every_tag(),
        every_offered_directive_carries_the_angular_documentation(),
        angular_elements_are_offered_as_tags_with_their_documentation(),
        a_plain_html_buffer_never_offers_the_angular_directives(),
    ]
}
