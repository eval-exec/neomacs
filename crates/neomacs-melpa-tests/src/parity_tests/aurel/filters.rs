use expect_test::expect;

use super::ParityBatchCase;

fn aurel_filter_chain_threads_mutations_and_continues_after_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_filter_chain_threads_mutations_and_continues_after_nil",
        r##"(let (events)
         (cl-labels
             ((add-first
               (info)
               (push :first events)
               (cons
                '(first . 1)
                info))
              (drop
               (_info)
               (push :drop events)
               nil)
              (must-not-run
               (info)
               (push :late events)
               info))
           (list
            (aurel-apply-filters
             '((original . 0))
             (list
              #'add-first))
            (aurel-apply-filters
             '((original . 0))
             (list
              #'add-first
              #'drop
              #'must-not-run))
            (nreverse events))))"##,
        expect![[r#"OK (((first . 1) (original . 0)) nil (:first :first :drop :late))"#]],
    )
}

fn aurel_contains_every_string_combines_selected_fields_literally() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_contains_every_string_combines_selected_fields_literally",
        r##"(let ((info
                '((name . "Emacs Git")
                  (description
                   . "Fast.Editor for Lisp"))))
         (mapcar
          (lambda (case)
            (let ((aurel-filter-params
                   (car case))
                  (aurel-filter-strings
                   (cdr case)))
              (let ((result
                     (aurel-filter-contains-every-string
                      info)))
                (list
                 (and result t)
                 (and result
                      (bui-entry-value
                       result
                       'name))))))
          '((nil "missing")
            ((name description))
            ((name description)
             "Emacs"
             "Editor")
            ((name description)
             "emacs")
            ((name description)
             "Fast.Editor")
            ((name)
             "Editor"))))"##,
        expect![[
            r#"OK ((t "Emacs Git") (t "Emacs Git") (t "Emacs Git") (t "Emacs Git") (t "Emacs Git") (nil nil))"#
        ]],
    )
}

fn aurel_aur_url_filters_mutate_package_path_and_prepend_git_url() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_aur_url_filters_mutate_package_path_and_prepend_git_url",
        r##"(let* ((info
                 '((name . "emacs-git")
                   (pkg-url
                    . "/cgit/aur.git/snapshot/emacs-git.tar.gz")
                   (id . 42)))
                (pkg-result
                 (aurel-filter-pkg-url
                  info))
                (git-result
                 (aurel-filter-git-url
                  pkg-result)))
         (list
          pkg-result
          git-result
          (eq info pkg-result)
          (eq pkg-result
              (cdr git-result))))"##,
        expect![[
            r#"OK (#1=((name . "emacs-git") (pkg-url . "https://aur.archlinux.org/cgit/aur.git/snapshot/emacs-git.tar.gz") (id . 42)) ((git-url . "https://aur.archlinux.org/emacs-git.git") . #1#) t t)"#
        ]],
    )
}

fn aurel_pacman_none_filter_normalizes_strings_and_rejects_non_string_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_pacman_none_filter_normalizes_strings_and_rejects_non_string_values",
        r##"(let ((aurel-none-string
                "None"))
         (list
          (aurel-pacman-filter-none
           '((installed-name . "demo")
             (depends-opt . "None")
             (required . "none")
             (optional-for . "")
             (validated . "SHA-256")))
          (aurel-test-error-data
           (lambda ()
             (aurel-pacman-filter-none
              '((installed-size . 0)))))))"##,
        expect![[
            r#"OK (((installed-name . "demo") (depends-opt) (required . "none") (optional-for . "") (validated . "SHA-256")) (:error wrong-type-argument (stringp 0)))"#
        ]],
    )
}

fn aurel_filtered_alist_keys_mutations_and_exposes_continuation_after_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_filtered_alist_keys_mutations_and_exposes_continuation_after_nil",
        r##"(let ((filters
                (list
                 (lambda (info)
                   (and
                    (alist-get
                     'keep
                     info)
                    info))
                 (lambda (info)
                   (cons
                    (cons
                     'processed
                     (upcase
                      (alist-get
                       'name
                       info)))
                    info)))))
         (list
          (aurel-get-filtered-alist
           '(((id . 10)
              (name . "alpha")
              (keep . t))
             ((id . 30)
              (name . "gamma")
              (keep . yes)))
           filters
           'id)
          (aurel-test-error-data
           (lambda ()
             (aurel-get-filtered-alist
              '(((id . 20)
                 (name . "drop")
                 (keep)))
              filters
              'id)))))"##,
        expect![[
            r#"OK (((10 (processed . "ALPHA") (id . 10) (name . "alpha") (keep . t)) (30 (processed . "GAMMA") (id . 30) (name . "gamma") (keep . yes))) (:error wrong-type-argument (char-or-string-p nil)))"#
        ]],
    )
}

fn aurel_receive_packages_joins_aur_and_pacman_records_by_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_receive_packages_joins_aur_and_pacman_records_by_name",
        r##"(let ((aurel-installed-packages-check
                t)
               calls)
         (cl-letf
             (((symbol-function
                'aurel-get-aur-packages-info)
               (lambda (url)
                 (push
                  (list :aur url)
                  calls)
                 '((("Name" . "alpha")
                    ("ID" . 10)
                    ("URLPath" . "/alpha.tar.gz"))
                   (("Name" . "beta")
                    ("ID" . 20)
                    ("URLPath" . "/beta.tar.gz")))))
              ((symbol-function
                'aurel-get-installed-packages-info)
               (lambda (&rest names)
                 (push
                  (cons :pacman names)
                  calls)
                 '((("Name" . "beta")
                    ("Version" . "2.0")
                    ("Optional Deps" . "None"))
                   (("Name" . "unrelated")
                    ("Version" . "9.0"))))))
           (list
            (aurel-receive-packages-info
             "fixture:aur")
            (nreverse calls))))"##,
        expect![[
            r#"OK (((10 (git-url . "https://aur.archlinux.org/alpha.git") (name . "alpha") (id . 10) (pkg-url . "https://aur.archlinux.org/alpha.tar.gz")) (20 (git-url . "https://aur.archlinux.org/beta.git") (name . "beta") (id . 20) (pkg-url . "https://aur.archlinux.org/beta.tar.gz") (installed-name . "beta") (installed-version . "2.0") (depends-opt))) ((:aur "fixture:aur") (:pacman "alpha" "beta")))"#
        ]],
    )
}

fn aurel_package_predicates_cover_maintenance_versions_and_regexps() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_package_predicates_cover_maintenance_versions_and_regexps",
        r##"(progn
         (defun aurel-test-predicate-summary
             (entry)
           (list
            (bui-entry-value
             entry
             'name)
            (and
             (aurel-package-maintained?
              entry)
             t)
            (aurel-package-unmaintained?
             entry)
            (and
             (aurel-package-outdated?
              entry)
             t)
            (aurel-package-not-outdated?
             entry)
            (aurel-package-same-versions?
             entry)
            (aurel-package-different-versions?
             entry)
            (and
             (aurel-package-matching-regexp?
              entry
              "git\\|editor")
             t)
            (aurel-package-not-matching-regexp?
             entry
             "^no-match$")))
         (mapcar
          #'aurel-test-predicate-summary
          '(((name . "emacs-git")
             (description . "Editor")
             (maintainer . "Alice")
             (outdated . 1700000000)
             (version . "30.1")
             (installed-version . "30.1"))
            ((name . "tiny")
             (description . "Terminal helper")
             (maintainer)
             (outdated)
             (version . "2")
             (installed-version . "1"))
            ((name . "unknown")
             (description . "")
             (version)
             (installed-version)))))"##,
        expect![[
            r#"OK (("emacs-git" t nil t nil t nil t t) ("tiny" nil t nil t nil t nil t) ("unknown" nil t nil t t nil nil t))"#
        ]],
    )
}

fn aurel_filter_commands_forward_predicates_prefixes_and_regexp_closures() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurel_filter_commands_forward_predicates_prefixes_and_regexp_closures",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'bui-enable-filter)
               (lambda (predicate arg)
                 (push
                  (list
                   (if
                       (symbolp predicate)
                       predicate
                     :closure)
                   arg
                   (and
                    (not
                     (symbolp predicate))
                    (funcall
                     predicate
                     '((name . "emacs-git")
                       (description
                        . "Editor")))))
                  calls)
                 :enabled))
              ((symbol-function 'read-regexp)
               (lambda (_prompt)
                 "git"))
              ((symbol-function
                'completing-read)
               (lambda (&rest _arguments)
                 "aurel-filter-outdated")))
           (let ((current-prefix-arg
                  '(4)))
             (list
              (aurel-filter-maintained nil)
              (aurel-filter-unmaintained
               '(4))
              (aurel-filter-outdated nil)
              (aurel-filter-not-outdated
               '-)
              (aurel-filter-same-versions
               nil)
              (aurel-filter-different-versions
               7)
              (aurel-filter-match-regexp
               nil)
              (aurel-filter-not-match-regexp
               t)
              (aurel-enable-filter
               :outer)
              (nreverse calls)))))"##,
        expect![[
            r#"OK (:enabled :enabled :enabled :enabled :enabled :enabled :enabled :enabled :enabled ((aurel-package-unmaintained? nil nil) (aurel-package-maintained? (4) nil) (aurel-package-not-outdated? nil nil) (aurel-package-outdated? - nil) (aurel-package-different-versions? nil nil) (aurel-package-same-versions? 7 nil) (:closure nil nil) (:closure t 6) (aurel-package-not-outdated? :outer nil)))"#
        ]],
    )
}

pub(super) fn filters_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurel_filter_chain_threads_mutations_and_continues_after_nil(),
        aurel_contains_every_string_combines_selected_fields_literally(),
        aurel_aur_url_filters_mutate_package_path_and_prepend_git_url(),
        aurel_pacman_none_filter_normalizes_strings_and_rejects_non_string_values(),
        aurel_filtered_alist_keys_mutations_and_exposes_continuation_after_nil(),
        aurel_receive_packages_joins_aur_and_pacman_records_by_name(),
        aurel_package_predicates_cover_maintenance_versions_and_regexps(),
        aurel_filter_commands_forward_predicates_prefixes_and_regexp_closures(),
    ]
}
