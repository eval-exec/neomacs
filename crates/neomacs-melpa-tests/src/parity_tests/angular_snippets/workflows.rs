use expect_test::expect;

use super::ParityBatchCase;

fn loading_the_package_registers_its_snippet_tree_with_yasnippet() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_the_package_registers_its_snippet_tree_with_yasnippet",
        r##"
(progn
  (yas-global-mode 1)
  (list
   ;; `angular-snippets-initialize' runs from the package's own
   ;; `eval-after-load', so simply loading it is enough.
   :registered-directory
   (and (seq-find (lambda (directory)
                    (string-match-p "angular-snippets" directory))
                  yas-snippet-dirs)
        t)
   ;; Every html snippet is filed under the same key, so typing "ng" offers
   ;; all 42 and yasnippet must ask which one.  The javascript ones each
   ;; have their own key and expand without a prompt.
   :html-keys (ngs-test-keys 'html-mode)
   :html-snippets (length (plist-get (ngs-test-snippet-directory 'html-mode)
                                     :files))
   :js-keys (ngs-test-keys 'js-mode)
   ;; Two of the four shipped directories are empty.  js2-mode inherits the
   ;; javascript snippets from js-mode anyway, so its directory earns
   ;; nothing; a web-mode user gets no snippets at all.
   :shipped-directories
   (mapcar (lambda (mode) (cons mode (ngs-test-snippet-directory mode)))
           '(js2-mode web-mode))))
"##,
        expect![[
            r#"OK (:registered-directory t :html-keys ("ng") :html-snippets 42 :js-keys ("$b" "$e" "$f" "$on" "$v" "$va" "$w" "ngc" "ngd" "ngfa" "ngfi" "ngm" "ngro" "ngrw" "ngrwr" "ngs" "ngw") :shipped-directories ((js2-mode :exists t :files nil) (web-mode :exists t :files nil)))"#
        ]],
    )
}

fn expanding_a_directive_in_a_real_html_buffer_inserts_it_and_documents_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "expanding_a_directive_in_a_real_html_buffer_inserts_it_and_documents_it",
        r##"
(progn
  (yas-global-mode 1)
  (with-temp-buffer
    (html-mode)
    (yas-minor-mode 1)
    (insert "<div ng")
    (let ((clicked (ngs-test-expand "ng-click")))
      (insert "save()")
      (let ((filled (buffer-substring-no-properties (point-min) (point-max))))
        (yas-exit-all-snippets)
        (erase-buffer)
        (insert "<div ng>")
        (goto-char (1- (point-max)))
        (let ((before-bracket (ngs-test-expand "ng-show")))
          (yas-exit-all-snippets)
          (erase-buffer)
          ;; Directly after another attribute, with no separator: the
          ;; expansion runs the snippet's `ng-snip/maybe-space-after-attr',
          ;; whose whole job is to keep the two apart.
          (insert "<div ngclass=\"card\">")
          (goto-char 8)
          (let ((before-text (ngs-test-expand "ng-hide")))
            (yas-exit-all-snippets)
            (erase-buffer)
            (insert "<div ng class=\"card\">")
            (goto-char 8)
            (let ((before-space (ngs-test-expand "ng-hide")))
              (yas-exit-all-snippets)
              (erase-buffer)
              ;; A snippet with two fields, the second defaulting from the
              ;; first.
              (insert "<li ng")
              (let ((repeated (ngs-test-expand "ng-repeat")))
                (yas-exit-all-snippets)
                (list :ng-click clicked
                      :ng-click-filled filled
                      :before-closing-bracket before-bracket
                      :before-another-attribute before-text
                      :before-a-space before-space
                      :ng-repeat repeated
                      ;; Called on its own the helper does exactly what it
                      ;; says, which is what makes the expansion above a
                      ;; defect rather than a design choice.
                      :helper-called-directly
                      (mapcar (lambda (following)
                                (with-temp-buffer
                                  (insert following)
                                  (goto-char (point-min))
                                  (ng-snip/maybe-space-after-attr)
                                  (buffer-substring-no-properties
                                   (point-min) (point-max))))
                              '("class=\"card\">" ">" "/>" " class=\"card\">"
                                "")))))))))))
"##,
        expect![[
            r#"OK (:ng-click (:expanded t :buffer "<div ng-click=\"\"" :point 16 :in-snippet t :echoed ("Eval the given expression when element is clicked.")) :ng-click-filled "<div ng-click=\"save()\"" :before-closing-bracket (:expanded t :buffer "<div ng-show=\"\">" :point 15 :in-snippet t :echoed ("Hides the element if the expression is falsy.")) :before-another-attribute (:expanded t :buffer "<div ng-hide=\"\"class=\"card\">" :point 15 :in-snippet t :echoed ("Hides the element if the expression is truthy.")) :before-a-space (:expanded t :buffer "<div ng-hide=\"\" class=\"card\">" :point 15 :in-snippet t :echoed ("Hides the element if the expression is truthy. [2 times]")) :ng-repeat (:expanded t :buffer "<li ng-repeat=\"thing in things\"" :point 16 :in-snippet t :echoed ("Repeats template for every item in a list.")) :helper-called-directly (" class=\"card\">" ">" "/>" " class=\"card\">" ""))"#
        ]],
    )
    .fresh_process()
}

fn expanding_a_scope_snippet_in_a_real_javascript_buffer_needs_no_prompt() -> ParityBatchCase {
    ParityBatchCase::value(
        "expanding_a_scope_snippet_in_a_real_javascript_buffer_needs_no_prompt",
        r##"
(progn
  (yas-global-mode 1)
  (with-temp-buffer
    (js-mode)
    (yas-minor-mode 1)
    ;; Each javascript key is unique, so `yas-expand' resolves it without
    ;; asking; the chooser in `ngs-test-expand' is never consulted.
    (insert "$b")
    (let ((broadcast (ngs-test-expand "$b")))
      (yas-exit-all-snippets)
      (erase-buffer)
      (insert "$w")
      (let ((watch (ngs-test-expand "$w")))
        (yas-exit-all-snippets)
        (erase-buffer)
        (insert "ngc")
        (let ((controller (ngs-test-expand "ngc")))
          (yas-exit-all-snippets)
          (erase-buffer)
          ;; "ngrw" is a prefix of "ngrwr" and both are routes; typing the
          ;; longer key gets the route that also has a `resolve' block.
          (insert "ngrwr")
          (let ((route-with-resolve (ngs-test-expand "ngrwr")))
            (yas-exit-all-snippets)
            (erase-buffer)
            (insert "ngrw")
            (let ((route (ngs-test-expand "ngrw")))
              (list :broadcast broadcast
                    :watch watch
                    :controller controller
                    :route route
                    :route-with-resolve route-with-resolve))))))))
"##,
        expect![[
            r#"OK (:broadcast (:expanded t :buffer "$scope.$broadcast(\"\", );\n" :point 20 :in-snippet t :echoed nil) :watch (:expanded t :buffer "$scope.$watch(\"\", function (newValue, oldValue) {\n    \n});" :point 16 :in-snippet t :echoed nil) :controller (:expanded t :buffer "controller('', function ($scope, ) {\n    \n});" :point 13 :in-snippet t :echoed nil) :route (:expanded t :buffer "$routeProvider.when(\"\", {\n    templateUrl: \"\",\n    controller: \"\"\n});\n" :point 22 :in-snippet t :echoed nil) :route-with-resolve (:expanded t :buffer "$routeProvider.when(\"\", {\n    templateUrl: \"\",\n    controller: \"\",\n    resolve: {\n    }\n});\n" :point 22 :in-snippet t :echoed nil))"#
        ]],
    )
}

fn showing_docs_at_point_echoes_first_and_browses_the_camel_cased_url_second() -> ParityBatchCase {
    ParityBatchCase::value(
        "showing_docs_at_point_echoes_first_and_browses_the_camel_cased_url_second",
        r##"
(with-temp-buffer
  (html-mode)
  (insert "<div ng-click=\"save()\" ng-options=\"o for o in os\">\n")
  (goto-char (point-min))
  (search-forward "ng-click")
  (backward-char 3)
  (let* ((timers-before (ngs-test-forget-timers))
         (first (ngs-test-show-docs))
         (second (ngs-test-show-docs))
         (browsed (copy-sequence ngs-test-browsed))
         ;; Each call schedules another ten-second timer without cancelling
         ;; the last, so count what this workflow added rather than what the
         ;; editor happens to have pending.
         (fired (ngs-test-run-forget-timers timers-before))
         (third (ngs-test-show-docs)))
    (goto-char (point-min))
    (search-forward "ng-options")
    (backward-char 4)
    (let ((indirect (ngs-test-show-docs)))
      (list :first-press first
            :second-press second
            :browsed browsed
            :forget-timers-added fired
            :after-forgetting third
            ;; `ng-options' documents itself but sends the reader to the
            ;; `select' page, which is the only entry whose URL is not its
            ;; own name.
            :indirected indirect
            :browsed-in-total (copy-sequence ngs-test-browsed)))))
"##,
        expect![[
            r#"OK (:first-press (:echoed ("Eval the given expression when element is clicked.") :signalled nil :remembered "ng-click") :second-press (:echoed nil :signalled nil :remembered "ng-click") :browsed ("http://docs.angularjs.org/api/ng.directive:ngClick") :forget-timers-added 1 :after-forgetting (:echoed ("Eval the given expression when element is clicked. [2 times]") :signalled nil :remembered "ng-click") :indirected (:echoed ("Populates select options from a list or object.") :signalled nil :remembered "ng-options") :browsed-in-total ("http://docs.angularjs.org/api/ng.directive:ngClick"))"#
        ]],
    )
}

fn every_directive_has_a_docstring_and_a_url_built_from_its_camel_cased_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_directive_has_a_docstring_and_a_url_built_from_its_camel_cased_name",
        r##"
(list :entries (length ng-docs)
      :root ng-snip/directive-root-url
      :indirections (ngs-test-plain ng-snip/docs-indirection)
      :docs (ngs-test-plain ng-docs))
"##,
        expect![[
            r#"OK (:entries 43 :root "http://docs.angularjs.org/api/ng.directive:" :indirections (("ng-options" . "select") ("ng-switch-when" . "ng-switch")) :docs (("ng-app" :docstring "Auto-bootstraps an application, with optional module to load." :docurl "http://docs.angularjs.org/api/ng.directive:ngApp") ("ng-bind" :docstring "Replace text content of element with value of given expression." :docurl "http://docs.angularjs.org/api/ng.directive:ngBind") ("ng-bind-html-unsafe" :docstring "Set innerHTML of element to unsanitized value of given expression." :docurl "http://docs.angularjs.org/api/ng.directive:ngBindHtmlUnsafe") ("ng-bind-template" :docstring "Replace text content of element with given template." :docurl "http://docs.angularjs.org/api/ng.directive:ngBindTemplate") ("ng-change" :docstring "Eval the given expression when user changes the input. Requires ng-model." :docurl "http://docs.angularjs.org/api/ng.directive:ngChange") ("ng-checked" :docstring "Uses given expression to determine checked-state of checkbox." :docurl "http://docs.angularjs.org/api/ng.directive:ngChecked") ("ng-class" :docstring "Sets class names on element based on given expression." :docurl "http://docs.angularjs.org/api/ng.directive:ngClass") ("ng-class-even" :docstring "Like ng-class, but only on even rows. Requires ng-repeat." :docurl "http://docs.angularjs.org/api/ng.directive:ngClassEven") ("ng-class-odd" :docstring "Like ng-class, but only on odd rows. Requires ng-repeat." :docurl "http://docs.angularjs.org/api/ng.directive:ngClassOdd") ("ng-click" :docstring "Eval the given expression when element is clicked." :docurl "http://docs.angularjs.org/api/ng.directive:ngClick") ("ng-cloak" :docstring "Hides the element contents until compiled by angular." :docurl "http://docs.angularjs.org/api/ng.directive:ngCloak") ("ng-controller" :docstring "Assign controller to this element, along with a new scope." :docurl "http://docs.angularjs.org/api/ng.directive:ngController") ("ng-csp" :docstring "Enables Content Security Policy support. Should be on same element as ng-app." :docurl "http://docs.angularjs.org/api/ng.directive:ngCsp") ("ng-dblclick" :docstring "Eval the given expression when element is double clicked." :docurl "http://docs.angularjs.org/api/ng.directive:ngDblclick") ("ng-disabled" :docstring "Uses given expression to determine disabled-state of element." :docurl "http://docs.angularjs.org/api/ng.directive:ngDisabled") ("ng-form" :docstring "Nestable alias of the form directive." :docurl "http://docs.angularjs.org/api/ng.directive:ngForm") ("ng-hide" :docstring "Hides the element if the expression is truthy." :docurl "http://docs.angularjs.org/api/ng.directive:ngHide") ("ng-href" :docstring "Avoids bad URLs on links that are clicked before angular compiles them." :docurl "http://docs.angularjs.org/api/ng.directive:ngHref") ("ng-include" :docstring "Fetches, compiles and includes an external HTML fragment." :docurl "http://docs.angularjs.org/api/ng.directive:ngInclude") ("ng-init" :docstring "Evals expression before executing template during bootstrap." :docurl "http://docs.angularjs.org/api/ng.directive:ngInit") ("ng-list" :docstring "Text input that converts between comma-separated string and an array of strings." :docurl "http://docs.angularjs.org/api/ng.directive:ngList") ("ng-model" :docstring "Sets up two-way data binding. Works with input, select and textarea." :docurl "http://docs.angularjs.org/api/ng.directive:ngModel") ("ng-mousedown" :docstring "Eval the given expression on mousedown." :docurl "http://docs.angularjs.org/api/ng.directive:ngMousedown") ("ng-mouseenter" :docstring "Eval the given expression on mouseenter." :docurl "http://docs.angularjs.org/api/ng.directive:ngMouseenter") ("ng-mouseleave" :docstring "Eval the given expression on mouseleave." :docurl "http://docs.angularjs.org/api/ng.directive:ngMouseleave") ("ng-mousemove" :docstring "Eval the given expression on mousemove." :docurl "http://docs.angularjs.org/api/ng.directive:ngMousemove") ("ng-mouseover" :docstring "Eval the given expression on mouseover." :docurl "http://docs.angularjs.org/api/ng.directive:ngMouseover") ("ng-mouseup" :docstring "Eval the given expression on mouseup." :docurl "http://docs.angularjs.org/api/ng.directive:ngMouseup") ("ng-multiple" :docstring "Uses given expression to determine multiple-state of select element." :docurl "http://docs.angularjs.org/api/ng.directive:ngMultiple") ("ng-non-bindable" :docstring "Makes angular ignore {{bindings}} inside element." :docurl "http://docs.angularjs.org/api/ng.directive:ngNonBindable") ("ng-options" :docstring "Populates select options from a list or object." :docurl "http://docs.angularjs.org/api/ng.directive:select") ("ng-pluralize" :docstring "Helps change wording based on a number." :docurl "http://docs.angularjs.org/api/ng.directive:ngPluralize") ("ng-readonly" :docstring "Uses given expression to determine readonly-state of element." :docurl "http://docs.angularjs.org/api/ng.directive:ngReadonly") ("ng-repeat" :docstring "Repeats template for every item in a list." :docurl "http://docs.angularjs.org/api/ng.directive:ngRepeat") ("ng-selected" :docstring "Uses given expression to determine selected-state of option element." :docurl "http://docs.angularjs.org/api/ng.directive:ngSelected") ("ng-show" :docstring "Hides the element if the expression is falsy." :docurl "http://docs.angularjs.org/api/ng.directive:ngShow") ("ng-src" :docstring "Stops browser from fetching images with {{templates}} in the URL." :docurl "http://docs.angularjs.org/api/ng.directive:ngSrc") ("ng-style" :docstring "Sets style attributes from an object of DOM style properties. " :docurl "http://docs.angularjs.org/api/ng.directive:ngStyle") ("ng-submit" :docstring "Eval the given expression when form is submitted, and prevent default." :docurl "http://docs.angularjs.org/api/ng.directive:ngSubmit") ("ng-switch" :docstring "Switch on given expression to conditionally change DOM structure." :docurl "http://docs.angularjs.org/api/ng.directive:ngSwitch") ("ng-switch-when" :docstring "Include this element if value matches ng-switch on expression." :docurl "http://docs.angularjs.org/api/ng.directive:ngSwitch") ("ng-transclude" :docstring "Signifies where to insert transcluded DOM." :docurl "http://docs.angularjs.org/api/ng.directive:ngTransclude") ("ng-view" :docstring "Signifies where route views are shown." :docurl "http://docs.angularjs.org/api/ng.directive:ngView")))"#
        ]],
    )
}

fn looking_for_a_directive_where_there_is_none_fails_three_different_ways() -> ParityBatchCase {
    ParityBatchCase::value(
        "looking_for_a_directive_where_there_is_none_fails_three_different_ways",
        r##"
(list
 ;; Past the last "ng-" in the buffer: `forward-char 3' runs off the end.
 :past-the-end
 (with-temp-buffer
   (html-mode)
   (insert "<div ng-click=\"save()\">")
   (goto-char (point-max))
   (ngs-test-show-docs))
 ;; Before the first "ng-": the backward search finds nothing.
 :before-any-directive
 (with-temp-buffer
   (html-mode)
   (insert "<div class=\"card\" ng-click=\"save()\">")
   (goto-char (point-min))
   (ngs-test-show-docs))
 ;; A "ng-" that is not a directive: the backward search finds it, and the
 ;; package's own check then rejects it.  This is the only one of the three
 ;; failures that reports itself in the package's own words.
 :not-a-directive
 (with-temp-buffer
   (html-mode)
   (insert "<div data-ng-9=\"legacy\"> and some following text\n")
   (goto-char (point-min))
   (search-forward "ng-9")
   (ngs-test-show-docs))
 ;; A directive the table does not know: `ng-snip/docs-value' returns nil
 ;; and `message' is called with it, which clears the echo area instead of
 ;; reporting anything.
 :unknown-directive
 (with-temp-buffer
   (html-mode)
   (insert "<div ng-sparkle=\"yes\">")
   (goto-char (point-min))
   (search-forward "ng-sparkle")
   (backward-char 4)
   (ngs-test-show-docs))
 ;; The package leaves two globals undeclared, so a caller cannot bind them
 ;; and lexical code that tries reads the global instead.
 :undeclared-globals
 (mapcar (lambda (symbol) (list symbol (and (boundp symbol) t)
                                (special-variable-p symbol)))
         '(ng-docs angular-snippets-root ng-snip/last-docs-message)))
"##,
        expect![[
            r#"OK (:past-the-end (:echoed nil :signalled (end-of-buffer) :remembered nil) :before-any-directive (:echoed nil :signalled (search-failed "ng-") :remembered nil) :not-a-directive (:echoed nil :signalled (error "No angular identifier at point") :remembered nil) :unknown-directive (:echoed nil :signalled nil :remembered "ng-sparkle") :undeclared-globals ((ng-docs t nil) (angular-snippets-root t nil) (ng-snip/last-docs-message t t)))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_the_package_registers_its_snippet_tree_with_yasnippet(),
        expanding_a_directive_in_a_real_html_buffer_inserts_it_and_documents_it(),
        expanding_a_scope_snippet_in_a_real_javascript_buffer_needs_no_prompt(),
        showing_docs_at_point_echoes_first_and_browses_the_camel_cased_url_second(),
        every_directive_has_a_docstring_and_a_url_built_from_its_camel_cased_name(),
        looking_for_a_directive_where_there_is_none_fails_three_different_ways(),
    ]
}
