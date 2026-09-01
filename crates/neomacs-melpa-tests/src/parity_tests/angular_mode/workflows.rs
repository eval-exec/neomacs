use expect_test::expect;

use super::ParityBatchCase;

fn opening_a_controller_highlights_the_angular_api_and_keeps_javascript_editing() -> ParityBatchCase
{
    ParityBatchCase::value(
        "opening_a_controller_highlights_the_angular_api_and_keeps_javascript_editing",
        r##"
        ;; A user opens an AngularJS controller and turns the mode on.  Every
        ;; core API call the package knows about has to pick up the builtin
        ;; face, and everything javascript-mode already did - strings,
        ;; keywords, parameter names - has to survive, because the mode is a
        ;; derivation and not a replacement.  `$scope' is the discriminator: it
        ;; is not in the package's service list, so it must still come back
        ;; with javascript-mode's variable face rather than the builtin one.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ang-test-visit "app/controllers.js"
                                             ang-test-controller-js #'angular-mode))
                (list
                 :mode major-mode
                 :derived-from (get 'angular-mode 'derived-mode-parent)
                 :mode-name (ang-test-copy mode-name)
                 :api-calls (ang-test-tokens-with-face 'font-lock-builtin-face)
                 :declaration-line (ang-test-faces-on-line 2)
                 :scope-is-not-a-package-keyword (ang-test-face-of "$scope")
                 :watch (ang-test-face-of "$watch")
                 :for-each (ang-test-face-of "angular.forEach")
                 :module (ang-test-face-of "angular.module")
                 :string-still-a-string (ang-test-face-of "'/api/widgets'")))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:mode angular-mode :derived-from javascript-mode :mode-name "JavaScript[Angular]" :api-calls ("$apply" "$broadcast" "$http" "$timeout" "$watch" ".controller" ".directive" "angular.copy" "angular.forEach" "angular.module") :declaration-line ((nil "  ") (font-lock-builtin-face ".controller") (nil "(") (font-lock-string-face "'WidgetCtrl'") (nil ", ") (font-lock-keyword-face "function") (nil " (") (font-lock-variable-name-face "$scope") (nil ", ") (font-lock-builtin-face "$http") (nil ", ") (font-lock-builtin-face "$timeout") (nil ") {")) :scope-is-not-a-package-keyword (:token "$scope" :face font-lock-variable-name-face :column 38 :line 2) :watch (:token "$watch" :face font-lock-builtin-face :column 11 :line 4) :for-each (:token "angular.forEach" :face font-lock-builtin-face :column 6 :line 5) :module (:token "angular.module" :face font-lock-builtin-face :column 0 :line 1) :string-still-a-string (:token "'/api/widgets'" :face font-lock-string-face :column 14 :line 9))"#
        ]],
    )
}

fn directive_properties_and_test_blocks_carry_the_type_face() -> ParityBatchCase {
    ParityBatchCase::value(
        "directive_properties_and_test_blocks_carry_the_type_face",
        r##"
        ;; The package paints two more groups with the type face: the property
        ;; names that make up a directive definition object, and the block
        ;; openers of a Mocha spec.  Both are written with their punctuation in
        ;; the keyword list - `scope:' with the colon, `describe(' with the
        ;; paren - so the punctuation is part of what is highlighted, and a
        ;; property that merely starts the same way is not.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ang-test-visit "app/controllers.js"
                                             ang-test-controller-js #'angular-mode))
                (list
                 :typed-tokens (ang-test-tokens-with-face 'font-lock-type-face)
                 :directive-object (list (ang-test-face-of "scope:")
                                         (ang-test-face-of "templateUrl:")
                                         (ang-test-face-of "transclude:")
                                         (ang-test-face-of "controllerAs:")
                                         (ang-test-face-of "link:"))
                 :spec-blocks (list (ang-test-face-of "describe(")
                                    (ang-test-face-of "beforeEach(")
                                    (ang-test-face-of "it("))
                 :directive-line (ang-test-faces-on-line 15)
                 :controller-keywords-are-empty angular-controller-definition-keywords))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:typed-tokens ("beforeEach(" "controllerAs:" "describe(" "it(" "link:" "scope:" "templateUrl:" "transclude:") :directive-object ((:token "scope:" :face font-lock-type-face :column 6 :line 16) (:token "templateUrl:" :face font-lock-type-face :column 6 :line 17) (:token "transclude:" :face font-lock-type-face :column 6 :line 18) (:token "controllerAs:" :face font-lock-type-face :column 6 :line 19) (:token "link:" :face font-lock-type-face :column 6 :line 20)) :spec-blocks ((:token "describe(" :face font-lock-type-face :column 0 :line 24) (:token "beforeEach(" :face font-lock-type-face :column 2 :line 25) (:token "it(" :face font-lock-type-face :column 2 :line 26)) :directive-line ((nil "    ") (font-lock-keyword-face "return") (nil " {")) :controller-keywords-are-empty nil)"#
        ]],
    )
}

fn the_keyword_lists_match_substrings_so_unrelated_identifiers_are_highlighted() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_keyword_lists_match_substrings_so_unrelated_identifiers_are_highlighted",
        r##"
        ;; The keyword lists go through `regexp-opt' with no word boundaries,
        ;; so they match wherever the text occurs and not only as whole tokens.
        ;; This fixture is ordinary JavaScript that happens to contain the
        ;; keywords as substrings, and the workflow pins where the highlighting
        ;; actually lands: `$id' inside `$idle' leaving `le' plain,
        ;; `.controller' inside `.controllers' leaving the `s' plain,
        ;; `.forEach' inside `forEachChild', and `angular.module' inside
        ;; `myangular.module' - a module call on a library that is not Angular
        ;; at all.  `describeTheWidget(' is the counter-case: the Mocha keyword
        ;; carries its own paren, so a longer name does not match it.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ang-test-visit "app/lookalikes.js"
                                             ang-test-lookalikes-js #'angular-mode))
                (list
                 :highlighted-as-api (ang-test-tokens-with-face 'font-lock-builtin-face)
                 :highlighted-as-type (ang-test-tokens-with-face 'font-lock-type-face)
                 :dollar-id-inside-idle (ang-test-faces-on-line 1)
                 :controller-inside-controllers (ang-test-faces-on-line 5)
                 :for-each-inside-for-each-child (ang-test-face-of "forEachChild")
                 :module-on-another-library (ang-test-faces-on-line 3)
                 :describe-needs-its-paren (ang-test-face-of "describeTheWidget(")))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:highlighted-as-api ("$id" ".controller" ".forEach" "angular.module") :highlighted-as-type nil :dollar-id-inside-idle ((font-lock-keyword-face "var") (nil " ") (font-lock-builtin-face "$id") (nil "le = ") (font-lock-constant-face "true") (nil ";")) :controller-inside-controllers ((nil "inventory") (font-lock-builtin-face ".controller") (nil "s.push(") (font-lock-string-face "'WidgetCtrl'") (nil ");")) :for-each-inside-for-each-child (:token "forEachChild" :face font-lock-builtin-face :column 8 :line 6) :module-on-another-library ((nil "my") (font-lock-builtin-face "angular.module") (nil "(") (font-lock-string-face "'fake'") (nil ");")) :describe-needs-its-paren (:token "describeTheWidget(" :face nil :column 0 :line 7))"#
        ]],
    )
}

fn an_angular_template_highlights_interpolations_only_where_html_has_not_claimed_them()
-> ParityBatchCase {
    ParityBatchCase::value(
        "an_angular_template_highlights_interpolations_only_where_html_has_not_claimed_them",
        r##"
        ;; `angular-html-mode' appends its two patterns after the sgml rules,
        ;; and font-lock lets the first matcher win, so what the user sees
        ;; depends on whether html-mode already claimed the text.  The fixture
        ;; carries one case of each outcome and the workflow compares the mode
        ;; against plain `html-mode' on identical text, so a rule that never
        ;; fires cannot be mistaken for one that works.
        ;;
        ;; The interpolation rule fires in a plain element and is shadowed
        ;; inside a heading, which already has a face.  The directive rule
        ;; never fires at all: `ng-repeat' and friends are painted by sgml's
        ;; attribute rule, identically in both modes.  And the mode loses
        ;; html-mode's doctype highlighting, which is the only other thing it
        ;; changes.
        (let ((angular nil) (plain nil))
          (unwind-protect
              (progn
                (require 'angular-html-mode)
                (setq angular (ang-test-visit "app/angular.html"
                                              ang-test-template-html #'angular-html-mode))
                (let ((angular-report
                       (list :mode major-mode
                             :derived-from (get 'angular-html-mode 'derived-mode-parent)
                             :mode-name (ang-test-copy mode-name)
                             :interpolation-in-a-plain-element
                             (ang-test-face-of "{{ widgets.length }}")
                             :interpolation-inside-a-heading
                             (ang-test-face-of "{{ ctrl.title }}")
                             :directive-attribute (ang-test-face-of "ng-repeat")
                             :doctype (ang-test-faces-on-line 1)
                             :own-keyword-face (ang-test-tokens-with-face
                                                'font-lock-keyword-face)))
                      (angular-faces (mapcar (lambda (position)
                                               (get-text-property position 'face))
                                             (number-sequence (point-min) (1- (point-max))))))
                  (setq plain (ang-test-visit "app/plain.html"
                                              ang-test-template-html #'html-mode))
                  (let ((plain-faces (mapcar (lambda (position)
                                               (get-text-property position 'face))
                                             (number-sequence (point-min) (1- (point-max))))))
                    (list
                     :angular angular-report
                     :plain-html (list :interpolation-in-a-plain-element
                                       (ang-test-face-of "{{ widgets.length }}")
                                       :directive-attribute (ang-test-face-of "ng-repeat")
                                       :doctype (ang-test-faces-on-line 1)
                                       :own-keyword-face (ang-test-tokens-with-face
                                                          'font-lock-keyword-face))
                     :characters-compared (length plain-faces)
                     :characters-that-differ
                     (cl-count-if-not #'identity
                                      (cl-mapcar #'equal angular-faces plain-faces))))))
            (when (buffer-live-p angular) (kill-buffer angular))
            (when (buffer-live-p plain) (kill-buffer plain))))
    "##,
        expect![[
            r#"OK (:angular (:mode angular-html-mode :derived-from html-mode :mode-name "HTML[Angular]" :interpolation-in-a-plain-element (:token "{{ widgets.length }}" :face font-lock-keyword-face :column 9 :line 13) :interpolation-inside-a-heading (:token "{{ ctrl.title }}" :face (bold underline) :column 8 :line 4) :directive-attribute (:token "ng-repeat" :face font-lock-variable-name-face :column 10 :line 6) :doctype ((font-lock-string-face "<!DOCTYPE html>")) :own-keyword-face ("{{ widgets.length }}")) :plain-html (:interpolation-in-a-plain-element (:token "{{ widgets.length }}" :face nil :column 9 :line 13) :directive-attribute (:token "ng-repeat" :face font-lock-variable-name-face :column 10 :line 6) :doctype ((nil "<") (font-lock-keyword-face "!DOCTYPE") (nil " html>")) :own-keyword-face ("!DOCTYPE")) :characters-compared 503 :characters-that-differ 35)"#
        ]],
    )
}

fn editing_behaviour_comes_from_the_parent_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "editing_behaviour_comes_from_the_parent_modes",
        r##"
        ;; Both modes are derivations, so the editing a user actually does -
        ;; indenting a block, commenting a region, moving over syntax - has to
        ;; keep working exactly as the parent provides it.  A package that
        ;; replaced rather than derived would show up here as an indentation
        ;; that does nothing or a comment syntax that inserts the wrong thing.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ang-test-visit "app/edit.js" "\
angular.module('inventory')
.controller('Ctrl', function ($scope) {
$scope.total = 0;
$scope.$watch('items', function (items) {
$scope.total = items.length;
});
});
" #'angular-mode))
                (list
                 :indented (progn (indent-region (point-min) (point-max))
                                  (buffer-substring-no-properties (point-min) (point-max)))
                 :comment-syntax (list comment-start comment-end)
                 :commented (progn (goto-char (point-min))
                                   (forward-line 2)
                                   (comment-region (line-beginning-position)
                                                   (line-end-position))
                                   (buffer-substring-no-properties
                                    (line-beginning-position) (line-end-position)))
                 :still-angular-after-editing
                 (progn (font-lock-ensure)
                        (ang-test-tokens-with-face 'font-lock-builtin-face))))
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:indented "angular.module('inventory')\n    .controller('Ctrl', function ($scope) {\n\11$scope.total = 0;\n\11$scope.$watch('items', function (items) {\n\11    $scope.total = items.length;\n\11});\n    });\n" :comment-syntax ("// " "") :commented "\11// $scope.total = 0;" :still-angular-after-editing ("$watch" ".controller" "angular.module"))"#
        ]],
    )
}

fn neither_mode_claims_a_file_extension_and_the_shipped_snippets_are_not_registered()
-> ParityBatchCase {
    ParityBatchCase::value(
        "neither_mode_claims_a_file_extension_and_the_shipped_snippets_are_not_registered",
        r##"
        ;; Both modes are autoloaded, so `M-x angular-mode' works without
        ;; requiring anything - but the package adds nothing to
        ;; `auto-mode-alist', so opening a `.js' or `.html' file gets the stock
        ;; mode and the user has to select Angular's explicitly.  The package
        ;; also ships a `snippets/' tree, and registers it nowhere: it is
        ;; inert until the user adds it to `yas-snippet-dirs' themselves.
        ;; Pinning both is the honest description of what installing this
        ;; package gets you.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ang-test-visit "app/plain.js" ang-test-controller-js
                                             #'normal-mode))
                (list
                 :javascript-file-gets-the-stock-mode major-mode
                 :template-file-gets-the-stock-mode
                 (let ((template (ang-test-visit "app/plain.html"
                                                 ang-test-template-html #'normal-mode)))
                   (unwind-protect major-mode (kill-buffer template)))
                 :auto-mode-alist-entries
                 (seq-filter (lambda (entry) (memq (cdr entry) '(angular-mode angular-html-mode)))
                             auto-mode-alist)
                 :both-modes-are-commands (list (commandp 'angular-mode)
                                                (commandp 'angular-html-mode))
                 :snippets-shipped
                 (let ((directory (expand-file-name
                                   "snippets"
                                   (file-name-directory (locate-library "angular-mode")))))
                   (list :exists (file-directory-p directory)
                         :trees (sort (seq-remove (lambda (name) (member name '("." "..")))
                                                  (directory-files directory))
                                      #'string<)
                         :names (sort (mapcar #'ang-test-copy
                                              (seq-remove
                                               (lambda (name) (member name '("." "..")))
                                               (directory-files
                                                (expand-file-name "angular-mode" directory))))
                                      #'string<)))
                 :snippets-registered (and (boundp 'yas-snippet-dirs) yas-snippet-dirs)))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:javascript-file-gets-the-stock-mode js-mode :template-file-gets-the-stock-mode mhtml-mode :auto-mode-alist-entries nil :both-modes-are-commands (t t) :snippets-shipped (:exists t :trees ("angular-html-mode" "angular-mode") :names ("config" "controller" "module" "stateprovider")) :snippets-registered nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_a_controller_highlights_the_angular_api_and_keeps_javascript_editing(),
        directive_properties_and_test_blocks_carry_the_type_face(),
        the_keyword_lists_match_substrings_so_unrelated_identifiers_are_highlighted(),
        an_angular_template_highlights_interpolations_only_where_html_has_not_claimed_them(),
        editing_behaviour_comes_from_the_parent_modes(),
        neither_mode_claims_a_file_extension_and_the_shipped_snippets_are_not_registered(),
    ]
}
