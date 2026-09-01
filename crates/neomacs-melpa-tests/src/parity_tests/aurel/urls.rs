use expect_test::expect;

use super::ParityBatchCase;

fn aurel_form_encodes_mixed_field_values_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_form_encodes_mixed_field_values_in_order",
        r##"(aurel-get-fields-string
         '(("plain" . "value")
           ("space" . "a b")
           ("symbol" . symbolic)
           ("number" . 42)
           ("nothing")
           ("unicode" . "λ/β?")))"##,
        expect![[
            r#"OK "plain=value&space=a%20b&symbol=symbolic&number=42&nothing=nil&unicode=%CE%BB%2F%CE%B2%3F""#
        ]],
    )
}

fn aurel_rpc_builder_covers_info_search_and_invalid_methods() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_rpc_builder_covers_info_search_and_invalid_methods",
        r##"(list
         (aurel-get-rpc-url
          "info"
          '(("arg[]" . "ripgrep")
            ("arg[]" . "emacs-git")))
         (aurel-get-rpc-url
          "search"
          '(("by" . "name-desc")
            ("arg" . "editor")))
         (aurel-test-error-data
          (lambda ()
            (aurel-get-rpc-url
             "delete"
             '(("arg" . "unsafe"))))))"##,
        expect![[
            r#"OK ("https://aur.archlinux.org/rpc/v5/info?arg[]=ripgrep&arg[]=emacs-git" "https://aur.archlinux.org/rpc/v5/search/editor?by=name-desc" (:error error ("Unknown search type: delete")))"#
        ]],
    )
}

fn aurel_package_info_url_preserves_repeated_arguments_and_escaping() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_package_info_url_preserves_repeated_arguments_and_escaping",
        r##"(mapcar
         (lambda (names)
           (apply
            #'aurel-get-package-info-url
            names))
         '(nil
           ("one")
           ("one" "two words" "c++")))"##,
        expect![[
            r#"OK ("https://aur.archlinux.org/rpc/v5/info?" "https://aur.archlinux.org/rpc/v5/info?arg[]=one" "https://aur.archlinux.org/rpc/v5/info?arg[]=one&arg[]=two%20words&arg[]=c%2B%2B")"#
        ]],
    )
}

fn aurel_search_url_helpers_select_exact_rpc_fields() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_search_url_helpers_select_exact_rpc_fields",
        r##"(list
         (aurel-get-package-search-url "common lisp")
         (aurel-get-package-search-url "clang++" "name")
         (aurel-get-package-name-search-url "emacs")
         (aurel-get-maintainer-search-url "alice@example")
         (aurel-get-maintainer-account-url "Alice Smith")
         (aurel-get-aur-package-url "emacs-git")
         (aurel-get-package-base-url "emacs")
         (aurel-get-package-action-url "emacs" "vote")
         (aurel-get-package-git-url "emacs-git")
         (aurel-get-package-cgit-url "emacs-git"))"##,
        expect![[
            r#"OK ("https://aur.archlinux.org/rpc/v5/search/commonlisp?by=name-desc" "https://aur.archlinux.org/rpc/v5/search/clang++?by=name" "https://aur.archlinux.org/rpc/v5/search/emacs?by=name" "https://aur.archlinux.org/rpc/v5/search/alice@example?by=maintainer" "https://aur.archlinux.org/account/AliceSmith" "https://aur.archlinux.org/packages/emacs-git" "https://aur.archlinux.org/pkgbase/emacs" "https://aur.archlinux.org/pkgbase/emacs/vote" "https://aur.archlinux.org/emacs-git.git" "https://aur.archlinux.org/cgit/aur.git/?h=emacs-git")"#
        ]],
    )
}

fn aurel_search_dispatch_forwards_each_public_search_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_search_dispatch_forwards_each_public_search_contract",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'aurel-get-packages-by-name)
               (lambda (&rest values)
                 (push
                  (cons :name values)
                  calls)
                 :by-name))
              ((symbol-function
                'aurel-get-packages-by-string)
               (lambda (&rest values)
                 (push
                  (cons :string values)
                  calls)
                 :by-string))
              ((symbol-function
                'aurel-get-packages-by-name-string)
               (lambda (&rest values)
                 (push
                  (cons :name-string values)
                  calls)
                 :by-name-string))
              ((symbol-function
                'aurel-get-packages-by-maintainer)
               (lambda (&rest values)
                 (push
                  (cons :maintainer values)
                  calls)
                 :by-maintainer)))
           (list
            (aurel-search-packages
             'name
             "one"
             "two")
            (aurel-search-packages
             'string
             "long phrase"
             "short")
            (aurel-search-packages
             'name-string
             "emacs")
            (aurel-search-packages
             'maintainer
             "alice")
            (aurel-test-error-data
             (lambda ()
               (aurel-search-packages
                'unsupported
                "value")))
            (nreverse calls))))"##,
        expect![[
            r#"OK (:by-name :by-string :by-name-string :by-maintainer (:error error ("Wrong search type ‘unsupported’")) ((:name "one" "two") (:string "long phrase" "short") (:name-string "emacs") (:maintainer "alice")))"#
        ]],
    )
}

fn aurel_public_search_commands_forward_real_user_inputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_public_search_commands_forward_real_user_inputs",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'aurel-search-show-packages)
               (lambda (&rest arguments)
                 (push arguments calls)
                 (cons :displayed arguments)))
              ((symbol-function
                'aurel-get-foreign-packages)
               (lambda ()
                 '("local-one"
                   "local-two"))))
           (list
            (aurel-package-info "exact-package")
            (aurel-package-search
             "font \"programming language\" terminal")
            (aurel-package-search-by-name
             "emacs")
            (aurel-maintainer-search
             "alice")
            (aurel-installed-packages)
            (nreverse calls))))"##,
        expect![[
            r#"OK ((:displayed . #1=(name "exact-package")) (:displayed . #2=(string "font" "programming language" "terminal")) (:displayed . #3=(name-string "emacs")) (:displayed . #4=(maintainer "alice")) (:displayed . #5=(name "local-one" "local-two")) (#1# #2# #3# #4# #5#))"#
        ]],
    )
}

fn aurel_multi_string_search_uses_longest_term_and_filters_the_rest() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_multi_string_search_uses_longest_term_and_filters_the_rest",
        r##"(let (captured)
         (cl-letf
             (((symbol-function
                'aurel-receive-packages-info)
               (lambda (url)
                 (setq captured
                       (list
                        url
                        aurel-filter-params
                        aurel-filter-strings))
                 :received)))
           (list
            (aurel-get-packages-by-string
             "tiny"
             "longest phrase"
             "medium")
            captured)))"##,
        expect![[
            r#"OK (:received ("https://aur.archlinux.org/rpc/v5/search/longestphrase?by=name-desc" (name description) ("medium" "tiny")))"#
        ]],
    )
}

fn aurel_get_package_wrappers_construct_url_then_receive_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_get_package_wrappers_construct_url_then_receive_once",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'aurel-receive-packages-info)
               (lambda (url)
                 (push url calls)
                 (list :received url))))
           (list
            (aurel-get-packages-by-name
             "one"
             "two")
            (aurel-get-packages-by-name-string
             "editor")
            (aurel-get-packages-by-maintainer
             "alice")
            (nreverse calls))))"##,
        expect![[
            r#"OK ((:received "https://aur.archlinux.org/rpc/v5/info?arg[]=one&arg[]=two") (:received "https://aur.archlinux.org/rpc/v5/search/editor?by=name") (:received "https://aur.archlinux.org/rpc/v5/search/alice?by=maintainer") ("https://aur.archlinux.org/rpc/v5/info?arg[]=one&arg[]=two" "https://aur.archlinux.org/rpc/v5/search/editor?by=name" "https://aur.archlinux.org/rpc/v5/search/alice?by=maintainer"))"#
        ]],
    )
}

pub(super) fn urls_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurel_form_encodes_mixed_field_values_in_order(),
        aurel_rpc_builder_covers_info_search_and_invalid_methods(),
        aurel_package_info_url_preserves_repeated_arguments_and_escaping(),
        aurel_search_url_helpers_select_exact_rpc_fields(),
        aurel_search_dispatch_forwards_each_public_search_contract(),
        aurel_public_search_commands_forward_real_user_inputs(),
        aurel_multi_string_search_uses_longest_term_and_filters_the_rest(),
        aurel_get_package_wrappers_construct_url_then_receive_once(),
    ]
}
