use expect_test::expect;

use super::ParityBatchCase;

fn asx_exact_package_descriptor_origin_dependency_and_feature_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_exact_package_descriptor_origin_dependency_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'asx package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'asx)
          (featurep 'request)
          (package-installed-p
           'asx
           '(20191024 1100))
          (file-name-nondirectory
           (locate-library "asx"))))"##,
        expect![[
            r#"OK (asx "20191024.1100" "Ask StackExchange/StackOverflow." nil ((emacs (26 1))) ((:maintainers ("Alex Ragone" . "ragonedk@gmail.com")) (:authors ("Alex Ragone" . "ragonedk@gmail.com")) (:keywords "convenience") (:revdesc . "5ca12cc51bb0") (:commit . "5ca12cc51bb02b5926adf9a7976ba9ca08a1ea21") (:url . "https://github.com/ragone/asx")) t t t "asx.el")"#
        ]],
    )
}

fn asx_installed_payload_inventory_sizes_and_content_digests_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_installed_payload_inventory_sizes_and_content_digests_match",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'asx package-alist)))
                 (directory
                  (package-desc-dir descriptor)))
         (mapcar
          (lambda (file)
            (let ((path
                   (expand-file-name file directory)))
              (list
               file
               (file-attribute-size
                (file-attributes path))
               (with-temp-buffer
                 (insert-file-contents-literally path)
                 (secure-hash
                  'sha256
                  (current-buffer))))))
          (sort
           (seq-filter
            (lambda (file)
              (file-regular-p
               (expand-file-name file directory)))
            (directory-files directory nil "\\`[^.]"))
           #'string<)))"##,
        expect![[
            r#"OK (("asx-autoloads.el" 790 "b7419b305ddd63857d1c39449539cd8b13e24d9b97ce376cae7f4594a46b3696") ("asx-pkg.el" 405 "301d7c695479a1d819cc0ebbe0d347d40b40d0c517772f6a56781b834f2c0986") ("asx.el" 14824 "220bc57d98d09d383624541baa458084799a6e544ea69cf58ec4f8bf626f24d9"))"#
        ]],
    )
}

fn asx_complete_callable_command_arglist_and_source_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_complete_callable_command_arglist_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "asx"
                  (symbol-name symbol))
                 (not
                  (string-suffix-p
                   "--inliner"
                   (symbol-name symbol)))
                 (not
                  (string-suffix-p
                   "--cmacro"
                   (symbol-name symbol)))
                 (fboundp symbol)
                 (let ((file
                        (symbol-file symbol 'defun)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "asx.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (commandp symbol)
             (interactive-form symbol)
             (prin1-to-string
              (help-function-arglist symbol t))
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right))))))"##,
        expect![[
            r#"OK ((asx t (interactive (list (asx--read-query))) "(query)" "asx.el") (asx--extract-links nil nil "(dom)" "asx.el") (asx--extract-links-duckduckgo nil nil "(dom)" "asx.el") (asx--extract-links-google nil nil "(dom)" "asx.el") (asx--filter-posts nil nil "(links)" "asx.el") (asx--finalize-buffer nil nil "nil" "asx.el") (asx--get-answers nil nil "(dom)" "asx.el") (asx--get-buffer nil nil "nil" "asx.el") (asx--get-current-post nil nil "nil" "asx.el") (asx--get-language-maybe nil nil "(node)" "asx.el") (asx--get-language-string nil nil "(class)" "asx.el") (asx--get-posts-with-prefix nil nil "(posts)" "asx.el") (asx--get-prefix nil nil "(post)" "asx.el") (asx--get-search-engine nil nil "nil" "asx.el") (asx--get-tags nil nil "(dom)" "asx.el") (asx--get-user-agent nil nil "nil" "asx.el") (asx--handle-search nil nil "(dom)" "asx.el") (asx--helm-search nil nil "nil" "asx.el") (asx--initial-input nil nil "nil" "asx.el") (asx--insert-answers nil nil "(answers)" "asx.el") (asx--insert-node nil nil "(node)" "asx.el") (asx--insert-post nil nil "(post)" "asx.el") (asx--insert-post-dom nil nil "(dom)" "asx.el") (asx--insert-question nil nil "(question)" "asx.el") (asx--insert-tags nil nil "(tags)" "asx.el") (asx--ivy-search nil nil "nil" "asx.el") (asx--map-node nil nil "(node)" "asx.el") (asx--normalize-post nil nil "(dom)" "asx.el") (asx--prepare-buffer nil nil "nil" "asx.el") (asx--query-construct nil nil "(query)" "asx.el") (asx--query-string nil nil "(query)" "asx.el") (asx--query-string-sites nil nil "nil" "asx.el") (asx--read-query nil nil "nil" "asx.el") (asx--remove-and-next nil nil "(url)" "asx.el") (asx--request nil nil "(url callback &optional error-callback)" "asx.el") (asx--request-post nil nil "(post)" "asx.el") (asx--select-post nil nil "(posts)" "asx.el") (asx--symbol-or-region nil nil "nil" "asx.el") (asx-first-post t (interactive nil) "nil" "asx.el") (asx-jump t (interactive nil) "nil" "asx.el") (asx-n-post nil nil "(n)" "asx.el") (asx-next-post t (interactive nil) "nil" "asx.el") (asx-previous-post t (interactive nil) "nil" "asx.el") (asx-reload-post t (interactive nil) "nil" "asx.el"))"#
        ]],
    )
}

fn asx_complete_declared_variable_defaults_custom_and_source_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_complete_declared_variable_defaults_custom_and_source_surface_matches",
        r##"(cl-labels
        ((stable
          (value)
          (cond
           ((and
             (functionp value)
             (not
              (symbolp value)))
            :function)
           ((consp value)
            (cons
             (stable
              (car value))
             (stable
              (cdr value))))
           ((vectorp value)
            (cons
             :vector
             (mapcar
              #'stable
              (append value nil))))
           (t value))))
       (let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "asx"
                  (symbol-name symbol))
                 (boundp symbol)
                 (let ((file
                        (symbol-file symbol 'defvar)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "asx.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (prin1-to-string
              (stable
               (default-value symbol)))
             (special-variable-p symbol)
             (local-variable-if-set-p symbol)
             (custom-variable-p symbol)
             (prin1-to-string
              (stable
               (get symbol 'custom-type)))
             (prin1-to-string
              (stable
               (get symbol 'custom-group)))
             (prin1-to-string
              (stable
               (get symbol 'safe-local-variable)))
             (file-name-nondirectory
              (symbol-file symbol 'defvar))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right)))))))"##,
        expect![[
            r#"OK ((asx--current-post-index "0" t nil nil "nil" "nil" "nil" "asx.el") (asx--posts "nil" t nil nil "nil" "nil" "nil" "asx.el") (asx--query-history "nil" t nil nil "nil" "nil" "nil" "asx.el") (asx--user-agents "(\"Mozilla/5.0 (Macintosh; Intel Mac OS X 10.7; rv:11.0) Gecko/20100101 Firefox/11.0\" \"Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:22.0) Gecko/20100 101 Firefox/22.0\" \"Mozilla/5.0 (Windows NT 6.1; rv:11.0) Gecko/20100101 Firefox/11.0\" \"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_7_4) AppleWebKit/536.5 (KHTML, like Gecko) Chrome/19.0.1084.46 Safari/536.5\" \"Mozilla/5.0 (Windows; Windows NT 6.1) AppleWebKit/536.5 (KHTML, like Gecko) Chrome/19.0.1084.46 Safari/536.5\")" t nil nil "nil" "nil" "nil" "asx.el") (asx-buffer-name "\"*AskStackExchange*\"" t nil ((funcall #'#[nil ("*AskStackExchange*") #1=(t)])) "string" "nil" "nil" "asx.el") (asx-number-of-answers "3" t nil ((funcall #'#[nil (3) #1#])) "number" "nil" "nil" "asx.el") (asx-prompt-post-p "nil" t nil ((funcall #'#[nil (nil) #1#])) "boolean" "nil" "nil" "asx.el") (asx-search-engine "google" t nil ((funcall #'#[nil ('google) #1#])) "symbol" "nil" "nil" "asx.el") (asx-search-engine-alist "((google :format \"https://www.google.com/search?q=%s\" :extract-fn #'asx--extract-links-google) (duckduckgo :format \"https://www.duckduckgo.com/?q=%s\" :extract-fn #'asx--extract-links-duckduckgo))" t nil ((funcall #'#[nil ('((google :format "https://www.google.com/search?q=%s" :extract-fn #'asx--extract-links-google) (duckduckgo :format "https://www.duckduckgo.com/?q=%s" :extract-fn #'asx--extract-links-duckduckgo))) #1#])) "(alist :key-type symbol :value-type plist)" "nil" "nil" "asx.el") (asx-sites "(\"stackoverflow.com\" \"stackexchange.com\" \"superuser.com\" \"serverfault.com\" \"askubuntu.com\")" t nil ((funcall #'#[nil ('("stackoverflow.com" "stackexchange.com" "superuser.com" "serverfault.com" "askubuntu.com")) #1#])) "list" "nil" "nil" "asx.el") (asx-skip-unanswered "t" t nil ((funcall #'#[nil (t) #1#])) "boolean" "nil" "nil" "asx.el"))"#
        ]],
    )
}

fn asx_custom_group_members_types_defaults_and_documentation_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asx_custom_group_members_types_defaults_and_documentation_match",
        r##"(list
         (get 'asx 'custom-group)
         (get 'asx 'group-documentation)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (default-value symbol)
             (get symbol 'custom-type)
             (get symbol 'custom-group)
             (documentation-property
              symbol
              'variable-documentation)))
          '(asx-number-of-answers
            asx-prompt-post-p
            asx-sites
            asx-search-engine
            asx-search-engine-alist
            asx-skip-unanswered
            asx-buffer-name))
         (documentation 'asx)
         (documentation 'asx-n-post))"##,
        expect![[
            r#"OK (((asx-number-of-answers custom-variable) (asx-prompt-post-p custom-variable) (asx-sites custom-variable) (asx-search-engine custom-variable) (asx-search-engine-alist custom-variable) (asx-skip-unanswered custom-variable) (asx-buffer-name custom-variable)) "Ask StackExchange." ((asx-number-of-answers 3 number nil "Answers to include.") (asx-prompt-post-p nil boolean nil "If non-nil, prompt for post to show.\nOtherwise show the first post.") (asx-sites ("stackoverflow.com" "stackexchange.com" "superuser.com" "serverfault.com" "askubuntu.com") list nil "Sites to search.") (asx-search-engine google symbol nil "Search engine to use.") (asx-search-engine-alist ((google :format "https://www.google.com/search?q=%s" :extract-fn #'asx--extract-links-google) (duckduckgo :format "https://www.duckduckgo.com/?q=%s" :extract-fn #'asx--extract-links-duckduckgo)) (alist :key-type symbol :value-type plist) nil "Alist of search engine configurations.") (asx-skip-unanswered t boolean nil "If non-nil, skip posts which have no answers.") (asx-buffer-name "*AskStackExchange*" string nil "Name of buffer to insert post.")) "Search for QUERY.\nIf a prefix argument is provided, the initial input will be the symbol at point." "Jump N steps in ‘asx--posts’ and insert the post.")"#
        ]],
    )
}

fn asx_generated_autoload_registers_only_the_primary_command_without_loading_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asx_generated_autoload_registers_only_the_primary_command_without_loading_source",
        r##"(list
         (featurep 'asx)
         (fboundp 'asx)
         (autoloadp
          (symbol-function 'asx))
         ;; Mask the installed package's own directory.  Spelling it out
         ;; pinned the harness's acquisition layout, so this expectation
         ;; broke when the cache moved from package-cache/ to the
         ;; revision-pinned source-install-cache/ -- a harness change
         ;; wearing the shape of a package regression.  What the assertion
         ;; is about is that the autoload points at the installed
         ;; package's own source file.
         (replace-regexp-in-string
          (regexp-quote
           (directory-file-name
            (file-name-directory
             (getenv "NEOMACS_PACKAGE_SOURCE"))))
          "[PACKAGE]"
          (symbol-file 'asx 'defun)
          t t)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)))
          '(asx-jump
            asx-next-post
            asx-previous-post
            asx-reload-post
            asx-first-post
            asx-n-post))
         (boundp 'asx-sites)
         (boundp 'asx--posts))"##,
        expect![[
            r#"OK (nil t t "[PACKAGE]/asx.el" ((asx-jump nil) (asx-next-post nil) (asx-previous-post nil) (asx-reload-post nil) (asx-first-post nil) (asx-n-post nil)) nil nil)"#
        ]],
    )
}

pub(super) fn registry_asx_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asx_exact_package_descriptor_origin_dependency_and_feature_contract_match(),
        asx_installed_payload_inventory_sizes_and_content_digests_match(),
        asx_complete_callable_command_arglist_and_source_surface_matches(),
        asx_complete_declared_variable_defaults_custom_and_source_surface_matches(),
        asx_custom_group_members_types_defaults_and_documentation_match(),
    ]
}

pub(super) fn registry_asx_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![asx_generated_autoload_registers_only_the_primary_command_without_loading_source()]
}
