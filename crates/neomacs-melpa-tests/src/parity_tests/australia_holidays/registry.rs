use expect_test::expect;

use super::ParityBatchCase;

fn australia_holidays_exact_package_descriptor_origin_and_dependency_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_exact_package_descriptor_origin_and_dependency_contract_match",
        r##"(let* ((descriptor
                                 (cadr
                                  (assq
                                   'australia-holidays
                                   package-alist)))
                                (extras
                                 (package-desc-extras descriptor))
                                ;; Mask the installed package's own
                                ;; directory.  Spelling it out pinned the
                                ;; harness's acquisition layout, so this
                                ;; expectation broke when the cache moved
                                ;; from package-cache/ to the
                                ;; revision-pinned source-install-cache/ --
                                ;; a harness change wearing the shape of a
                                ;; package regression.
                                (installed
                                 (directory-file-name
                                  (file-name-directory
                                   (getenv
                                    "NEOMACS_PACKAGE_SOURCE")))))
                           (list
                            (package-desc-name descriptor)
                            (package-version-join
                             (package-desc-version descriptor))
                            (package-desc-summary descriptor)
                            (mapcar
                             (lambda (requirement)
                               (list
                                (car requirement)
                                (package-version-join
                                 (cadr requirement))))
                             (package-desc-reqs descriptor))
                            (package-desc-kind descriptor)
                            (package-desc-archive descriptor)
                            (replace-regexp-in-string
                             (regexp-quote installed)
                             "[PACKAGE]"
                             (package-desc-dir descriptor)
                             t t)
                            (alist-get :commit extras)
                            (alist-get :revdesc extras)
                            (alist-get :url extras)
                            (alist-get :keywords extras)
                            (alist-get :authors extras)
                            (alist-get :maintainers extras)))"##,
        expect![[
            r#"OK (australia-holidays "20250706.1213" "Australian holidays for calendar." ((emacs "24.1")) nil nil "[PACKAGE]" "a73bbc940bc953164b8ed77e61e65a7a3aff4da5" "a73bbc940bc9" "https://github.com/jmibanez/australia-holidays.el" ("calendar") (("JM Ibañez" . "jm@jmibanez.com")) (("JM Ibañez" . "jm@jmibanez.com")))"#
        ]],
    )
}

fn australia_holidays_installed_payload_inventory_and_exact_archive_hashes_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "australia_holidays_installed_payload_inventory_and_exact_archive_hashes_match",
        r##"(let* ((directory
                                 (file-name-directory
                                  (getenv
                                   "NEOMACS_PACKAGE_SOURCE")))
                                (archive-files
                                 '("australia-holidays-pkg.el"
                                   "australia-holidays.el")))
                           (mapcar
                            (lambda (file)
                              (let ((path
                                     (expand-file-name
                                      file
                                      directory)))
                                (if
                                    (member file archive-files)
                                    (list
                                     file
                                     :archive
                                     (file-attribute-size
                                      (file-attributes path))
                                     (with-temp-buffer
                                       (insert-file-contents-literally path)
                                       (secure-hash
                                        'sha256
                                        (current-buffer))))
                                  (list
                                   file
                                   :generated
                                   (file-readable-p path)))))
                            (sort
                             (seq-filter
                              (lambda (file)
                                (file-regular-p
                                 (expand-file-name
                                  file
                                  directory)))
                              (directory-files
                               directory
                               nil
                               "\\`[^.]"))
                             #'string<)))"##,
        expect![[
            r#"OK (("australia-holidays-autoloads.el" :generated t) ("australia-holidays-pkg.el" :archive 430 "c6c4fd234e27f0f8bbd52d482ba1295b259a46c87db30f7298a4bd1ec266c6d4") ("australia-holidays.el" :archive 5909 "ad2e5eb3bcbcef956f624107ab6bd8904218f20f4a3ab647627bf85b36217bf4") ("australia-holidays.elc" :generated t))"#
        ]],
    )
}

fn australia_holidays_complete_customization_defaults_and_metadata_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_complete_customization_defaults_and_metadata_match",
        r##"(mapcar
                          (lambda (symbol)
                            (list
                             symbol
                             (symbol-value symbol)
                             (get symbol 'custom-type)
                             (get symbol 'standard-value)
                             (get symbol 'custom-group)
                             (get symbol 'custom-requests)
                             (documentation-property
                              symbol
                              'variable-documentation
                              t)))
                          '(australia-holidays-january-26-label
                            australia-holidays-include-january-26))"##,
        expect![[
            r#"OK ((australia-holidays-january-26-label "Australia Day" string ((funcall #'#[nil ("Australia Day") #1=(t)])) nil nil "What to call the holiday celebrated on January 26.") (australia-holidays-include-january-26 t boolean ((funcall #'#[nil (t) #1#])) nil nil "Whether to include January 26 in the list of holidays."))"#
        ]],
    )
}

fn australia_holidays_all_national_state_and_territory_rule_forms_match_exactly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "australia_holidays_all_national_state_and_territory_rule_forms_match_exactly",
        r##"(mapcar
                          (lambda (symbol)
                            (list
                             symbol
                             (length
                              (symbol-value symbol))
                             (copy-tree
                              (symbol-value symbol))
                             (documentation-property
                              symbol
                              'variable-documentation
                              t)))
                          '(australia-holidays
                            australia-holidays-for-act
                            australia-holidays-for-nsw
                            australia-holidays-for-nt
                            australia-holidays-for-qld
                            australia-holidays-for-sa
                            australia-holidays-for-tas
                            australia-holidays-for-vic
                            australia-holidays-for-wa))"##,
        expect![[
            r#"OK ((australia-holidays 6 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day")) "Australian holidays.\nOnly provides holidays that are valid in all states and territories.") (australia-holidays-for-act 8 (australia-holidays (holiday-float 3 1 2 "Canberra Day") (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 5 1 1 "Reconciliation Day" 26) (holiday-float 6 1 2 "King's Birthday") (holiday-float 10 1 1 "Labour Day") (holiday-fixed 12 26 "Boxing Day")) "Holidays in the Australian Capital Territory.") (australia-holidays-for-nsw 12 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day") (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-fixed 4 25 "ANZAC Day") (holiday-float 6 1 2 "King's Birthday") (holiday-float 10 1 1 "Labour Day") (holiday-fixed 12 26 "Boxing Day")) "Holidays in New South Wales.") (australia-holidays-for-nt 14 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day") (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 5 1 1 "May Day") (holiday-float 6 1 2 "King's Birthday") (holiday-float 8 1 1 "Picnic Day") (holiday-fixed 12 24 "Christmas Eve") (holiday-fixed 12 26 "Boxing Day") (holiday-fixed 12 31 "New Year's Eve")) "Holidays in the Northern Territory.") (australia-holidays-for-qld 13 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day") (holiday-easter-etc -1 "The Day After Good Friday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 5 1 1 "Labour Day") (holiday-float 8 3 1 "Royal Queensland Show" 9) (holiday-float 10 1 1 "King's Birthday") (holiday-fixed 12 24 "Christmas Eve") (holiday-fixed 12 26 "Boxing Day")) "Holidays in Queensland.") (australia-holidays-for-sa 14 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day") (holiday-float 3 1 2 "Adelaide Cup Day") (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 6 1 2 "King's Birthday") (holiday-float 10 1 1 "Labour Day") (holiday-fixed 12 24 "Christmas Eve") (holiday-fixed 12 26 "Proclamation Day") (holiday-fixed 12 31 "New Year's Eve")) "Holidays in South Australia.") (australia-holidays-for-tas 12 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day") (holiday-float 2 1 2 "Royal Hobart Regatta") (holiday-float 3 1 2 "Eight Hours Day") (holiday-easter-etc 2 "Easter Tuesday") (holiday-float 6 1 2 "King's Birthday") (holiday-float 11 1 1 "Recreation Day") (holiday-fixed 12 26 "Boxing Day")) "Holidays in Tasmania.") (australia-holidays-for-vic 13 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day") (holiday-float 3 1 2 "Labour Day") (holiday-easter-etc -1 "Saturday Before Easter Sunday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 6 1 2 "King's Birthday") (holiday-float 11 2 1 "Melbourne Cup") (holiday-float 9 5 -1 "Friday before AFL Grand Final" 29) (holiday-fixed 12 26 "Boxing Day")) "Holidays in Victoria.") (australia-holidays-for-wa 10 ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day") (holiday-float 3 1 1 "Labour Day") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 6 1 1 "Western Australia Day") (holiday-fixed 12 26 "Boxing Day")) "Holidays in Western Australia."))"#
        ]],
    )
}

fn australia_holidays_regional_lists_preserve_exact_symbolic_and_shared_element_topology()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_regional_lists_preserve_exact_symbolic_and_shared_element_topology",
        r##"(let ((regional-symbols
                                '(australia-holidays-for-nsw
                                  australia-holidays-for-nt
                                  australia-holidays-for-qld
                                  australia-holidays-for-sa
                                  australia-holidays-for-tas
                                  australia-holidays-for-vic
                                  australia-holidays-for-wa)))
                           (list
                            (car australia-holidays-for-act)
                            (listp
                             (car
                              australia-holidays-for-act))
                            (mapcar
                             (lambda (symbol)
                               (let ((regional
                                      (symbol-value symbol)))
                                 (list
                                  symbol
                                  (eq regional australia-holidays)
                                  (eq
                                   (car regional)
                                   (car australia-holidays))
                                  (eq
                                   (nth 1 regional)
                                   (nth 1 australia-holidays))
                                  (equal
                                   (seq-take
                                    regional
                                    (length australia-holidays))
                                   australia-holidays))))
                             regional-symbols)))"##,
        expect![
            "OK (australia-holidays nil ((australia-holidays-for-nsw nil t t t) (australia-holidays-for-nt nil t t t) (australia-holidays-for-qld nil t t t) (australia-holidays-for-sa nil t t t) (australia-holidays-for-tas nil t t t) (australia-holidays-for-vic nil t t t) (australia-holidays-for-wa nil t t t)))"
        ],
    )
}

fn australia_holidays_source_reload_preserves_every_user_assignment_and_feature_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_source_reload_preserves_every_user_assignment_and_feature_state",
        r##"(let ((source
                                (getenv
                                 "NEOMACS_PACKAGE_SOURCE")))
                           (setq
                            australia-holidays-january-26-label
                            "User Day"
                            australia-holidays-include-january-26
                            :user-choice
                            australia-holidays
                            '((holiday-fixed 7 7 "User National"))
                            australia-holidays-for-act
                            '((holiday-fixed 8 8 "User ACT"))
                            australia-holidays-for-vic
                            '((holiday-fixed 9 9 "User VIC")))
                           (load source nil t t)
                           (list
                            australia-holidays-january-26-label
                            australia-holidays-include-january-26
                            australia-holidays
                            australia-holidays-for-act
                            australia-holidays-for-vic
                            (featurep
                             'australia-holidays)))"##,
        expect![[
            r#"OK ("User Day" :user-choice ((holiday-fixed 7 7 "User National")) ((holiday-fixed 8 8 "User ACT")) ((holiday-fixed 9 9 "User VIC")) t)"#
        ]],
    )
}

fn australia_holidays_generated_autoloads_define_all_options_and_rule_lists_without_loading_package()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_generated_autoloads_define_all_options_and_rule_lists_without_loading_package",
        r##"(list
                          (featurep
                           'australia-holidays-autoloads)
                          (featurep
                           'australia-holidays)
                          (mapcar
                           (lambda (symbol)
                             (list
                              symbol
                              (boundp symbol)
                              (and
                               (boundp symbol)
                               (symbol-value symbol))
                              (get symbol 'custom-type)))
                           '(australia-holidays-january-26-label
                             australia-holidays-include-january-26
                             australia-holidays
                             australia-holidays-for-act
                             australia-holidays-for-nsw
                             australia-holidays-for-nt
                             australia-holidays-for-qld
                             australia-holidays-for-sa
                             australia-holidays-for-tas
                             australia-holidays-for-vic
                             australia-holidays-for-wa))
                          ;; Mask the installed package's own directory;
                          ;; see the note in the descriptor case above.
                          (mapcar
                           (lambda (entry)
                             (replace-regexp-in-string
                              (regexp-quote
                               (directory-file-name
                                (file-name-directory
                                 (getenv
                                  "NEOMACS_PACKAGE_SOURCE"))))
                              "[PACKAGE]"
                              entry
                              t t))
                           (seq-filter
                            (lambda (entry)
                              (string-match-p
                               "australia-holidays"
                               entry))
                            load-path))
                          (get
                           'australia-holidays
                           'definition-prefixes))"##,
        expect![[
            r#"OK (t nil ((australia-holidays-january-26-label t "Australia Day" nil) (australia-holidays-include-january-26 t t nil) (australia-holidays t (#1=(holiday-fixed 1 1 "New Year") #2=(if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) #3=(holiday-easter-etc -2 "Good Friday") #4=(holiday-easter-etc 1 "Easter Monday") #5=(holiday-fixed 4 25 "ANZAC Day") #6=(holiday-fixed 12 25 "Christmas Day")) nil) (australia-holidays-for-act t (australia-holidays (holiday-float 3 1 2 "Canberra Day") (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 5 1 1 "Reconciliation Day" 26) (holiday-float 6 1 2 "King's Birthday") (holiday-float 10 1 1 "Labour Day") (holiday-fixed 12 26 "Boxing Day")) nil) (australia-holidays-for-nsw t (#1# #2# #3# #4# #5# #6# (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-fixed 4 25 "ANZAC Day") (holiday-float 6 1 2 "King's Birthday") (holiday-float 10 1 1 "Labour Day") (holiday-fixed 12 26 "Boxing Day")) nil) (australia-holidays-for-nt t (#1# #2# #3# #4# #5# #6# (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 5 1 1 "May Day") (holiday-float 6 1 2 "King's Birthday") (holiday-float 8 1 1 "Picnic Day") (holiday-fixed 12 24 "Christmas Eve") (holiday-fixed 12 26 "Boxing Day") (holiday-fixed 12 31 "New Year's Eve")) nil) (australia-holidays-for-qld t (#1# #2# #3# #4# #5# #6# (holiday-easter-etc -1 "The Day After Good Friday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 5 1 1 "Labour Day") (holiday-float 8 3 1 "Royal Queensland Show" 9) (holiday-float 10 1 1 "King's Birthday") (holiday-fixed 12 24 "Christmas Eve") (holiday-fixed 12 26 "Boxing Day")) nil) (australia-holidays-for-sa t (#1# #2# #3# #4# #5# #6# (holiday-float 3 1 2 "Adelaide Cup Day") (holiday-easter-etc -1 "Easter Saturday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 6 1 2 "King's Birthday") (holiday-float 10 1 1 "Labour Day") (holiday-fixed 12 24 "Christmas Eve") (holiday-fixed 12 26 "Proclamation Day") (holiday-fixed 12 31 "New Year's Eve")) nil) (australia-holidays-for-tas t (#1# #2# #3# #4# #5# #6# (holiday-float 2 1 2 "Royal Hobart Regatta") (holiday-float 3 1 2 "Eight Hours Day") (holiday-easter-etc 2 "Easter Tuesday") (holiday-float 6 1 2 "King's Birthday") (holiday-float 11 1 1 "Recreation Day") (holiday-fixed 12 26 "Boxing Day")) nil) (australia-holidays-for-vic t (#1# #2# #3# #4# #5# #6# (holiday-float 3 1 2 "Labour Day") (holiday-easter-etc -1 "Saturday Before Easter Sunday") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 6 1 2 "King's Birthday") (holiday-float 11 2 1 "Melbourne Cup") (holiday-float 9 5 -1 "Friday before AFL Grand Final" 29) (holiday-fixed 12 26 "Boxing Day")) nil) (australia-holidays-for-wa t (#1# #2# #3# #4# #5# #6# (holiday-float 3 1 1 "Labour Day") (holiday-easter-etc 0 "Easter Sunday") (holiday-float 6 1 1 "Western Australia Day") (holiday-fixed 12 26 "Boxing Day")) nil)) ("[PACKAGE]") nil)"#
        ]],
    )
}

pub(super) fn registry_australia_holidays_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        australia_holidays_exact_package_descriptor_origin_and_dependency_contract_match(),
        australia_holidays_installed_payload_inventory_and_exact_archive_hashes_match(),
        australia_holidays_complete_customization_defaults_and_metadata_match(),
        australia_holidays_all_national_state_and_territory_rule_forms_match_exactly(),
        australia_holidays_regional_lists_preserve_exact_symbolic_and_shared_element_topology(),
        australia_holidays_source_reload_preserves_every_user_assignment_and_feature_state(),
    ]
}

pub(super) fn registry_australia_holidays_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        australia_holidays_generated_autoloads_define_all_options_and_rule_lists_without_loading_package(),
    ]
}
