use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_sage_exact_descriptor_provenance_and_dependency_graph_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_exact_descriptor_provenance_and_dependency_graph_match",
        r##"(mapcar
                           (lambda (package)
                             (let ((descriptor
                                    (cadr
                                     (assq package
                                           package-alist))))
                               (list
                                (package-desc-name descriptor)
                                (package-version-join
                                 (package-desc-version descriptor))
                                (package-desc-summary descriptor)
                                (package-desc-reqs descriptor)
                                (package-desc-extras descriptor))))
                           '(auto-complete-sage
                             auto-complete
                             popup
                             sage-shell-mode
                             deferred
                             let-alist))"##,
        expect![[
            r#"OK ((auto-complete-sage "20160514.751" "An auto-complete source for sage-shell-mode." ((auto-complete (1 5 1)) (sage-shell-mode (0 1 0))) ((:maintainers ("Sho Takemori" . "stakemorii@gmail.com")) (:authors ("Sho Takemori" . "stakemorii@gmail.com")) (:keywords "sage" "math" "auto-complete") (:revdesc . "51b8e3905196") (:commit . "51b8e3905196d266e1f8aa47881189833151b398") (:url . "https://github.com/stakemori/auto-complete-sage"))) (auto-complete "20251231.1622" "Auto Completion for GNU Emacs." ((emacs (25 1)) (popup (0 5 8))) ((:maintainers ("Jen-Chieh Shen" . "jcs090218@gmail.com")) (:authors ("Tomohiro Matsuyama" . "m2ym.pub@gmail.com")) (:keywords "completion" "convenience") (:revdesc . "07f9915e0834") (:commit . "07f9915e08342410b933145d7934998709753a29") (:url . "https://github.com/auto-complete/auto-complete"))) (popup "20251231.1622" "Visual Popup User Interface." ((emacs (24 3))) ((:maintainers ("Jen-Chieh" . "jcs090218@gmail.com")) (:authors ("Tomohiro Matsuyama" . "m2ym.pub@gmail.com")) (:keywords "lisp") (:revdesc . "45a0b759076c") (:commit . "45a0b759076ce4139aba36dde0a2904136282e73") (:url . "https://github.com/auto-complete/popup-el"))) (sage-shell-mode "20260523.1504" "A front-end for Sage Math." ((cl-lib (0 6 1)) (emacs (24 4)) (let-alist (1 0 5)) (deferred (0 5 1))) ((:maintainers ("Sho Takemori" . "stakemorii@gmail.com")) (:authors ("Sho Takemori" . "stakemorii@gmail.com")) (:keywords "sage" "math") (:revdesc . "bb59cd559a9d") (:commit . "bb59cd559a9d7639d9ef16addbb0809ea4790392") (:url . "https://github.com/sagemath/sage-shell-mode"))) (deferred "20170901.1330" "Simple asynchronous functions for emacs lisp." ((emacs (24 4))) ((:maintainers ("SAKURAI Masashi" . "m.sakuraiatkiwanami.net")) (:authors ("SAKURAI Masashi" . "m.sakuraiatkiwanami.net")) (:keywords "deferred" "async") (:revdesc . "2239671d94b3") (:commit . "2239671d94b38d92e9b28d4e12fd79814cfb9c16") (:url . "https://github.com/kiwanami/emacs-deferred"))) (let-alist "1.0.6" "Easily let-bind values of an assoc-list by their names" ((emacs (24 1))) ((:keywords "extensions" "lisp") (:maintainer "Artur Malabarba" . "emacs@endlessparentheses.com") (:authors ("Artur Malabarba" . "emacs@endlessparentheses.com")) (:url . "https://elpa.gnu.org/packages/let-alist.html") (:commit . "77fb84e6db96cbaa70e230f4881e4ede6e028f15"))))"#
        ]],
    )
}

fn auto_complete_sage_installed_payload_inventory_and_exact_hashes_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_installed_payload_inventory_and_exact_hashes_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'auto-complete-sage
                                   package-alist)))
                                (directory
                                 (package-desc-dir descriptor))
                                (files
                                 (sort
                                  (directory-files
                                   directory
                                   nil
                                   "auto-complete-sage.*\\.el\\'")
                                  #'string<)))
                           (mapcar
                            (lambda (name)
                              (let ((file
                                     (expand-file-name
                                      name
                                      directory)))
                                (list
                                 name
                                 (file-attribute-size
                                  (file-attributes file))
                                 (with-temp-buffer
                                   (set-buffer-multibyte nil)
                                   (insert-file-contents-literally
                                    file)
                                   (secure-hash
                                    'sha256
                                    (current-buffer))))))
                            files))"##,
        expect![[
            r#"OK (("auto-complete-sage-autoloads.el" 811 "163b98f46c8daced51515454bf96bd46fe712f586cff50742d2669d07593f73e") ("auto-complete-sage-pkg.el" 512 "87e381c69529050e1f42580807a4b70e67ad4e1e2a659ffda21057fd32894374") ("auto-complete-sage.el" 12854 "fb8ad7ce54536f3b9ff6b12a1b9666595d1d7daaf587455454567de0ef613f0c"))"#
        ]],
    )
}

fn auto_complete_sage_complete_target_symbol_inventory_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_complete_target_symbol_inventory_matches",
        r##"(let (symbols)
                           (mapatoms
                            (lambda (symbol)
                              (let ((name
                                     (symbol-name symbol))
                                    (origin
                                     (symbol-file symbol)))
                                (when
                                    (and
                                     (or
                                      (string-prefix-p
                                       "ac-sage"
                                       name)
                                      (string-prefix-p
                                       "ac-source-sage"
                                       name)
                                      (string-prefix-p
                                       "as-source-sage"
                                       name)
                                      (string=
                                       name
                                       "auto-complete-sage"))
                                     (equal
                                      (file-name-nondirectory
                                       (or origin ""))
                                      "auto-complete-sage.el"))
                                  (push
                                   (list
                                    symbol
                                    (fboundp symbol)
                                    (boundp symbol)
                                    (and
                                     (macrop symbol)
                                     t)
                                    (and
                                     (commandp symbol)
                                     t)
                                    (and
                                     (local-variable-if-set-p
                                      symbol)
                                     t)
                                    (file-name-nondirectory
                                     origin))
                                   symbols)))))
                           (sort
                            symbols
                            (lambda (left right)
                              (string<
                               (symbol-name
                                (car left))
                               (symbol-name
                                (car right))))))"##,
        expect![[
            r#"OK ((ac-sage--cache-doc t nil t nil nil "auto-complete-sage.el") (ac-sage--doc t nil nil nil nil "auto-complete-sage.el") (ac-sage--doc-clear-cache t nil nil nil nil "auto-complete-sage.el") (ac-sage--repl-methods-cached nil t nil nil t "auto-complete-sage.el") (ac-sage--sage-commands-doc-cached nil t nil nil t "auto-complete-sage.el") (ac-sage--sage-commands-doc-clear-cache t nil nil nil nil "auto-complete-sage.el") (ac-sage-complete-on-dot nil t nil nil nil "auto-complete-sage.el") (ac-sage-doc t nil nil nil nil "auto-complete-sage.el") (ac-sage-edit--modules-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-edit--sage-commands-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-edit--vars-in-module-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-edit:-source-base t nil t nil nil "auto-complete-sage.el") (ac-sage-edit:-state-cached nil t nil nil nil "auto-complete-sage.el") (ac-sage-edit:candidates t nil nil nil nil "auto-complete-sage.el") (ac-sage-quick-help-ignore-classes nil t nil nil nil "auto-complete-sage.el") (ac-sage-repl--argspec-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--attributes-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--base-name-and-name t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--modules-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--other-interface-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--sage-interface-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--vars-in-module-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl-methods-doc t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl-modules nil t nil nil nil "auto-complete-sage.el") (ac-sage-repl-python-kwds-candidates t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl-vars-in-module nil t nil nil nil "auto-complete-sage.el") (ac-sage-repl:-source-base t nil t nil nil "auto-complete-sage.el") (ac-sage-repl:add-sources t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl:candidates t nil nil nil nil "auto-complete-sage.el") (ac-sage-repl:python-kwds nil t nil nil nil "auto-complete-sage.el") (ac-sage-setup t nil nil t nil "auto-complete-sage.el") (ac-sage-show-quick-help nil t nil nil nil "auto-complete-sage.el") (ac-sage:add-sources t nil nil nil nil "auto-complete-sage.el") (ac-sage:complete-on-dot-prefix t nil nil nil nil "auto-complete-sage.el") (ac-sage:words-in-sage-buffers t nil nil nil nil "auto-complete-sage.el") (ac-source-sage-commands nil t nil nil nil "auto-complete-sage.el") (ac-source-sage-methods nil t nil nil nil "auto-complete-sage.el") (ac-source-sage-modules nil t nil nil nil "auto-complete-sage.el") (ac-source-sage-other-interfaces nil t nil nil nil "auto-complete-sage.el") (ac-source-sage-repl-python-kwds nil t nil nil nil "auto-complete-sage.el") (ac-source-sage-vars-in-modules nil t nil nil nil "auto-complete-sage.el") (ac-source-sage-words-in-buffers nil t nil nil nil "auto-complete-sage.el") (as-source-sage-repl-argspec nil t nil nil nil "auto-complete-sage.el") (auto-complete-sage nil nil nil nil nil "auto-complete-sage.el"))"#
        ]],
    )
}

fn auto_complete_sage_every_callable_arglist_interactivity_documentation_and_origin_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_every_callable_arglist_interactivity_documentation_and_origin_match",
        r##"(let (functions)
                           (mapatoms
                            (lambda (symbol)
                              (when
                                  (and
                                   (fboundp symbol)
                                   (equal
                                    (file-name-nondirectory
                                     (or
                                      (symbol-file
                                       symbol
                                       'defun)
                                      ""))
                                    "auto-complete-sage.el"))
                                (push
                                 (list
                                  symbol
                                  (help-function-arglist
                                   symbol t)
                                  (and
                                   (macrop symbol)
                                   t)
                                  (and
                                   (interactive-form symbol)
                                   t)
                                  (and
                                   (commandp symbol)
                                   t)
                                  (documentation symbol t)
                                  (file-name-nondirectory
                                   (symbol-file
                                    symbol
                                    'defun)))
                                 functions))))
                           (sort
                            functions
                            (lambda (left right)
                              (string<
                               (symbol-name
                                (car left))
                               (symbol-name
                                (car right))))))"##,
        expect![[
            r#"OK ((ac-sage--cache-doc (doc-func name base-name cache-var &rest --cl-rest--) t nil nil "\n\n(fn DOC-FUNC NAME BASE-NAME CACHE-VAR &optional (MIN-LEN 0))" "auto-complete-sage.el") (ac-sage--doc (name base-name) nil nil nil nil "auto-complete-sage.el") (ac-sage--doc-clear-cache nil nil nil nil nil "auto-complete-sage.el") (ac-sage--sage-commands-doc-clear-cache nil nil nil nil nil "auto-complete-sage.el") (ac-sage-doc (can) nil nil nil nil "auto-complete-sage.el") (ac-sage-edit--modules-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-edit--sage-commands-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-edit--vars-in-module-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-edit:-source-base (&rest --cl-rest--) t nil nil "\n\n(fn &key TYPE NAME (PRED t) (USE-CACHE t) (PREFIX-FUN \\='ac-prefix-default))" "auto-complete-sage.el") (ac-sage-edit:candidates nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--argspec-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--attributes-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--base-name-and-name (can) nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--modules-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--other-interface-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--sage-interface-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl--vars-in-module-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl-methods-doc (can) nil nil nil nil "auto-complete-sage.el") (ac-sage-repl-python-kwds-candidates nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl:-source-base (&rest --cl-rest--) t nil nil "\n\n(fn &key TYPE NAME (PRED t) (USE-CACHE t) (PREFIX-FUN \\='ac-prefix-default))" "auto-complete-sage.el") (ac-sage-repl:add-sources nil nil nil nil nil "auto-complete-sage.el") (ac-sage-repl:candidates (keys) nil nil nil nil "auto-complete-sage.el") (ac-sage-setup nil nil t t nil "auto-complete-sage.el") (ac-sage:add-sources nil nil nil nil nil "auto-complete-sage.el") (ac-sage:complete-on-dot-prefix nil nil nil nil nil "auto-complete-sage.el") (ac-sage:words-in-sage-buffers nil nil nil nil nil "auto-complete-sage.el"))"#
        ]],
    )
}

fn auto_complete_sage_custom_alias_cache_keyword_and_source_contracts_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_custom_alias_cache_keyword_and_source_contracts_match",
        r##"(list
                           (mapcar
                            (lambda (symbol)
                              (list
                               symbol
                               (default-value symbol)
                               (get symbol 'custom-type)
                               (get symbol 'custom-group)
                               (documentation-property
                                symbol
                                'variable-documentation)
                               (and
                                (local-variable-if-set-p
                                 symbol)
                                t)
                               (indirect-variable symbol)))
                            '(ac-sage-show-quick-help
                              ac-sage-complete-on-dot
                              ac-sage-quick-help-ignore-classes
                              ac-sage--repl-methods-cached
                              ac-sage--sage-commands-doc-cached
                              ac-sage-edit:-state-cached))
                           (list
                            (length
                             ac-sage-repl:python-kwds)
                            (car
                             ac-sage-repl:python-kwds)
                            (car
                             (last
                              ac-sage-repl:python-kwds))
                            (secure-hash
                             'sha256
                             (prin1-to-string
                              ac-sage-repl:python-kwds)))
                           (mapcar
                            (lambda (source)
                              (list
                               source
                               (mapcar
                                (lambda (entry)
                                  (let ((value
                                         (cdr entry)))
                                    (cons
                                     (car entry)
                                     (cond
                                      ((functionp value)
                                       :function)
                                      ((symbolp value)
                                       value)
                                      (t value)))))
                                (symbol-value source))))
                            '(ac-source-repl-sage-commands
                              ac-source-sage-methods
                              ac-source-sage-other-interfaces
                              ac-sage-repl-modules
                              ac-sage-repl-vars-in-module
                              ac-source-sage-repl-python-kwds
                              as-source-sage-repl-argspec
                              ac-source-sage-commands
                              ac-source-sage-modules
                              ac-source-sage-vars-in-modules
                              ac-source-sage-words-in-buffers)))"##,
        expect![[
            r#"OK (((ac-sage-show-quick-help nil boolean nil "Non-nil means show quick help of auto-complete-mode in\n‘sage-shell-mode’ buffers and ‘sage-shell:sage-mode’ buffers." nil ac-sage-show-quick-help) (ac-sage-complete-on-dot nil boolean nil "Non-nil means ‘auto-complete’ starts when dot is inserted." nil ac-sage-complete-on-dot) (ac-sage-quick-help-ignore-classes nil nil nil "If non-nil, this should be a list of strings.\nEach string should be a class of Sage. When non-nil instances or methods\nof these classes are ignored by ‘ac-quick-help’ and ‘eldoc’.\nIf the value is equal to ’(\"\"), then it does not ignore anything." nil sage-shell:inspect-ingnore-classes) (ac-sage--repl-methods-cached nil nil nil nil t ac-sage--repl-methods-cached) (ac-sage--sage-commands-doc-cached nil nil nil nil t ac-sage--sage-commands-doc-cached) (ac-sage-edit:-state-cached nil nil nil nil nil ac-sage-edit:-state-cached)) (110 "abs" "__import__" "fd9516441fa9e88f853a4ba64a4a1934c37104995b38704b9589dc37147692d7") ((ac-source-repl-sage-commands ((init . :function) (candidates . :function) (cache) (prefix . :function) (document . :function) (symbol . "s"))) (ac-source-sage-methods ((init . :function) (candidates . :function) (cache) (prefix . :function) (symbol . "s") (requires . 0) (document . :function))) (ac-source-sage-other-interfaces ((init . :function) (candidates . :function) (cache) (prefix . :function) (symbol . "s"))) (ac-sage-repl-modules ((symbol . "m") (requires . 0) (init . :function) (candidates . :function) (cache) (prefix . :function))) (ac-sage-repl-vars-in-module ((symbol . "s") (init . :function) (candidates . :function) (cache) (prefix . :function))) (ac-source-sage-repl-python-kwds ((candidates . :function))) (as-source-sage-repl-argspec ((init . :function) (candidates . :function) (cache) (prefix . :function))) (ac-source-sage-commands ((init . :function) (candidates . :function) (cache) (prefix . :function) (document . :function) (symbol . "s"))) (ac-source-sage-modules ((symbol . "m") (requires . 0) (init . :function) (candidates . :function) (cache) (prefix . :function))) (ac-source-sage-vars-in-modules ((symbol . "s") (init . :function) (candidates . :function) (cache) (prefix . :function))) (ac-source-sage-words-in-buffers ((init . :function) (candidates . :function)))))"#
        ]],
    )
}

fn auto_complete_sage_load_history_records_complete_contract_and_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_load_history_records_complete_contract_and_side_effects",
        r##"(let* ((file
                                 (locate-library
                                  "auto-complete-sage"))
                                (history
                                 (cdr
                                  (assoc file
                                         load-history))))
                           (list
                            (file-name-nondirectory file)
                            (seq-filter
                             (lambda (event)
                               (and
                                (consp event)
                                (memq
                                 (car event)
                                 '(require
                                   defun
                                   defvar
                                   defvaralias
                                   provide))))
                             history)
                            sage-shell:completion-function
                            (mapcar
                             (lambda (mode)
                               (list
                                mode
                                (memq mode ac-modes)
                                (cl-count mode ac-modes)))
                             '(sage-shell-mode
                               sage-shell:sage-mode))
                            (memq
                             'ac-sage--sage-commands-doc-clear-cache
                             sage-shell:clear-command-cache-hook)
                            (and
                             (boundp
                              'eldoc-message-commands)
                             (let ((commands
                                    eldoc-message-commands))
                               (list
                                (type-of commands)
                                (if
                                    (eq
                                     (type-of commands)
                                     'obarray)
                                    (and
                                     (intern-soft
                                      "ac-complete"
                                      commands)
                                     t)
                                  (and
                                   (memq
                                    'ac-complete
                                    commands)
                                   t))
                                (if
                                    (eq
                                     (type-of commands)
                                     'obarray)
                                    (and
                                     (intern-soft
                                      "ac-expand"
                                      commands)
                                     t)
                                  (and
                                   (memq
                                    'ac-expand
                                    commands)
                                   t)))))
                            (featurep
                             'auto-complete-sage)))"##,
        expect![[
            r#"OK ("auto-complete-sage.el" ((require . auto-complete) (require . sage-shell-mode) (defun . ac-sage--sage-commands-doc-clear-cache) (defun . ac-sage--doc-clear-cache) (require . help) (defun . ac-sage--cache-doc) (defun . ac-sage-doc) (defun . ac-sage-repl--base-name-and-name) (defun . ac-sage-repl-methods-doc) (defun . ac-sage--doc) (defun . ac-sage-repl:-source-base) (defun . ac-sage-repl--sage-interface-prefix) (defun . ac-sage-repl--attributes-prefix) (defun . ac-sage-repl:candidates) (defun . ac-sage-repl--other-interface-prefix) (defun . ac-sage-repl--modules-prefix) (defun . ac-sage-repl--vars-in-module-prefix) (defun . ac-sage-repl-python-kwds-candidates) (defun . ac-sage-repl--argspec-prefix) (defun . ac-sage-repl:add-sources) (defun . ac-sage-edit:-source-base) (defun . ac-sage-edit:candidates) (defun . ac-sage-edit--sage-commands-prefix) (defun . ac-sage-edit--modules-prefix) (defun . ac-sage:complete-on-dot-prefix) (defun . ac-sage-edit--vars-in-module-prefix) (defun . ac-sage:add-sources) (defun . ac-sage:words-in-sage-buffers) (defun . ac-sage-setup) (provide . auto-complete-sage)) auto-complete ((sage-shell-mode #1=(sage-shell-mode emacs-lisp-mode lisp-mode lisp-interaction-mode slime-repl-mode nim-mode c-mode cc-mode c++-mode objc-mode swift-mode go-mode java-mode malabar-mode clojure-mode clojurescript-mode scala-mode scheme-mode ocaml-mode tuareg-mode coq-mode haskell-mode agda-mode agda2-mode perl-mode cperl-mode python-mode ruby-mode lua-mode tcl-mode ecmascript-mode javascript-mode js-mode js-jsx-mode js2-mode js2-jsx-mode coffee-mode php-mode css-mode scss-mode less-css-mode elixir-mode makefile-mode sh-mode fortran-mode f90-mode ada-mode xml-mode sgml-mode web-mode ts-mode sclang-mode verilog-mode qml-mode apples-mode) 1) (sage-shell:sage-mode (sage-shell:sage-mode . #1#) 1)) (ac-sage--sage-commands-doc-clear-cache) (obarray t t) t)"#
        ]],
    )
}

fn auto_complete_sage_source_reload_reapplies_side_effects_without_overwriting_user_defvars()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_source_reload_reapplies_side_effects_without_overwriting_user_defvars",
        r##"(let ((ac-sage-show-quick-help
                                'user-quick-help)
                               (ac-source-sage-methods
                                'user-source)
                               (sage-shell:completion-function
                                'completion-at-point))
                           (setq ac-modes
                                 (delete
                                  'sage-shell-mode
                                  (delete
                                   'sage-shell:sage-mode
                                   ac-modes)))
                           (load
                            (getenv
                             "NEOMACS_PACKAGE_SOURCE")
                            nil t t)
                           (list
                            ac-sage-show-quick-help
                            ac-source-sage-methods
                            sage-shell:completion-function
                            (mapcar
                             (lambda (mode)
                               (list
                                mode
                                (cl-count
                                 mode
                                 ac-modes)))
                             '(sage-shell-mode
                               sage-shell:sage-mode))
                            (cl-count
                             'ac-sage--sage-commands-doc-clear-cache
                             sage-shell:clear-command-cache-hook)))"##,
        expect![
            "OK (user-quick-help user-source auto-complete ((sage-shell-mode 1) (sage-shell:sage-mode 1)) 1)"
        ],
    )
}

fn auto_complete_sage_generated_autoload_contains_only_setup_and_feature_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_generated_autoload_contains_only_setup_and_feature_contract",
        r##"(let* ((file
                                 (locate-library
                                  "auto-complete-sage-autoloads"))
                                (history
                                 (cdr
                                  (assoc file
                                         load-history))))
                           (list
                            (featurep
                             'auto-complete-sage-autoloads)
                            (featurep
                             'auto-complete-sage)
                            (fboundp
                             'ac-sage-setup)
                            (autoloadp
                             (symbol-function
                              'ac-sage-setup))
                            (file-name-nondirectory
                             (or
                              (symbol-file
                               'ac-sage-setup
                               'defun)
                              ""))
                            (seq-filter
                             (lambda (event)
                               (memq
                                (car-safe event)
                                '(defun
                                  defvar
                                  provide)))
                             history)))"##,
        expect![[
            r#"OK (t nil t t "auto-complete-sage.el" ((defun . ac-sage-setup) (provide . auto-complete-sage-autoloads)))"#
        ]],
    )
}

pub(super) fn registry_auto_complete_sage_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_sage_exact_descriptor_provenance_and_dependency_graph_match(),
        auto_complete_sage_installed_payload_inventory_and_exact_hashes_match(),
        auto_complete_sage_complete_target_symbol_inventory_matches(),
        auto_complete_sage_every_callable_arglist_interactivity_documentation_and_origin_match(),
        auto_complete_sage_custom_alias_cache_keyword_and_source_contracts_match(),
        auto_complete_sage_load_history_records_complete_contract_and_side_effects(),
        auto_complete_sage_source_reload_reapplies_side_effects_without_overwriting_user_defvars(),
    ]
}

pub(super) fn registry_auto_complete_sage_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_sage_generated_autoload_contains_only_setup_and_feature_contract()]
}
