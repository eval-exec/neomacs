use expect_test::expect;

use super::ParityBatchCase;

fn readme_preference_migrates_real_use_site_alists_and_their_rendered_icons() -> ParityBatchCase {
    ParityBatchCase::value(
        "readme_preference_migrates_real_use_site_alists_and_their_rendered_icons",
        r##"(progn
         (defvar all-the-icons-nerd-fonts-test-use-sites nil)
         (let ((all-the-icons-nerd-fonts-test-use-sites
                '((rust all-the-icons-alltheicon "rust" :face error)
                  (generic all-the-icons-faicon "address-book")
                  (github all-the-icons-faicon "github" :height 1.2)
                  (material all-the-icons-material "star")
                  (untouched all-the-icons-fileicon "unknown")))
               (all-the-icons-nerd-fonts-advise-all-the-icons-functions
                nil))
           (list
            (all-the-icons-nerd-fonts-prefer
             '(all-the-icons-nerd-fonts-test-use-sites))
            all-the-icons-nerd-fonts-test-use-sites
            (mapcar
             (lambda (entry)
               (let ((icon (apply (cadr entry) (cddr entry))))
                 (list
                  (car entry)
                  (substring-no-properties icon)
                  (string-to-list icon)
                  (get-text-property 0 'face icon)
                  (get-text-property 0 'display icon))))
             (butlast all-the-icons-nerd-fonts-test-use-sites))
            all-the-icons-nerd-fonts--advice-enabled
            all-the-icons-nerd-fonts--override-map)))"##,
        expect![[
            r#"OK (t ((rust all-the-icons-nerd-dev "rust" :face error) (generic all-the-icons-nerd-fa "address-book") (github all-the-icons-nerd-cod "github" :height 1.2) (material all-the-icons-nerd-md "star") (untouched all-the-icons-fileicon "unknown")) ((rust "" (59304) (:family "Symbols Nerd Font" :height 1.2 :inherit error) (raise -0.24)) (generic "" (62137) (:family "Symbols Nerd Font" :height 1.2) (raise -0.24)) (github "" (60036) (:family "Symbols Nerd Font" :height 1.44) (raise -0.24)) (material "󰓎" (984270) (:family "Symbols Nerd Font" :height 1.2) (raise -0.24))) nil nil)"#
        ]],
    )
    .fresh_process()
}

fn preference_is_idempotent_after_it_has_rewritten_real_associations() -> ParityBatchCase {
    ParityBatchCase::value(
        "preference_is_idempotent_after_it_has_rewritten_real_associations",
        r##"(progn
         (defvar all-the-icons-nerd-fonts-test-idempotent nil)
         (let ((all-the-icons-nerd-fonts-test-idempotent
                '((rust all-the-icons-alltheicon "rust")
                  (github all-the-icons-faicon "github")
                  (material all-the-icons-material "star")
                  (generic all-the-icons-faicon "address-book")))
               (all-the-icons-nerd-fonts-advise-all-the-icons-functions
                nil))
           (all-the-icons-nerd-fonts-prefer
            '(all-the-icons-nerd-fonts-test-idempotent))
           (let ((once
                  (copy-tree
                   all-the-icons-nerd-fonts-test-idempotent)))
             (all-the-icons-nerd-fonts-prefer
              '(all-the-icons-nerd-fonts-test-idempotent))
             (list
              once
              all-the-icons-nerd-fonts-test-idempotent
              (equal
               once
               all-the-icons-nerd-fonts-test-idempotent)))))"##,
        expect![[
            r#"OK (((rust all-the-icons-nerd-dev "rust") (github all-the-icons-nerd-cod "github") (material all-the-icons-nerd-md "star") (generic all-the-icons-nerd-fa "address-book")) ((rust all-the-icons-nerd-dev "rust") (github all-the-icons-nerd-cod "github") (material all-the-icons-nerd-md "star") (generic all-the-icons-nerd-fa "address-book")) t)"#
        ]],
    )
}

fn advice_configuration_changes_direct_calls_but_not_alist_migration() -> ParityBatchCase {
    ParityBatchCase::value(
        "advice_configuration_changes_direct_calls_but_not_alist_migration",
        r##"(progn
         (defvar all-the-icons-nerd-fonts-test-advice-site nil)
         (mapcar
          (lambda (enabled)
            (all-the-icons-nerd-fonts-unprefer)
            (let ((all-the-icons-nerd-fonts-test-advice-site
                   '((github all-the-icons-faicon "github")))
                  (all-the-icons-nerd-fonts-advise-all-the-icons-functions
                   enabled)
                  (all-the-icons-nerd-fonts--override-map nil)
                  (all-the-icons-nerd-fonts--advice-enabled nil))
              (let ((before (all-the-icons-faicon "github")))
                (all-the-icons-nerd-fonts-prefer
                 '(all-the-icons-nerd-fonts-test-advice-site))
                (let ((after (all-the-icons-faicon "github")))
                  (prog1
                      (list
                       enabled
                       all-the-icons-nerd-fonts-test-advice-site
                       (list
                        (substring-no-properties before)
                        (plist-get
                         (get-text-property 0 'face before)
                         :family))
                       (list
                        (substring-no-properties after)
                        (plist-get
                         (get-text-property 0 'face after)
                         :family))
                       all-the-icons-nerd-fonts--advice-enabled
                       (and
                        (advice-member-p
                         'all-the-icons-nerd-fonts
                         'all-the-icons-faicon)
                        t))
                    (all-the-icons-nerd-fonts-unprefer))))))
          '(nil t)))"##,
        expect![[
            r#"OK ((nil #1=((github all-the-icons-nerd-cod "github")) ("" "FontAwesome") ("" "FontAwesome") nil nil) (t #1# ("" "FontAwesome") ("" "Symbols Nerd Font") t t))"#
        ]],
    )
}

fn config_checker_distinguishes_valid_missing_skipped_unknown_and_unbound_sites() -> ParityBatchCase
{
    ParityBatchCase::value(
        "config_checker_distinguishes_valid_missing_skipped_unknown_and_unbound_sites",
        r##"(progn
         (defvar all-the-icons-nerd-fonts-test-valid-icons nil)
         (defvar all-the-icons-nerd-fonts-test-missing-icons nil)
         (defvar all-the-icons-nerd-fonts-test-skipped-icons nil)
         (defvar all-the-icons-nerd-fonts-test-unknown-family nil)
         (let ((all-the-icons-nerd-fonts--alist-vars
                '(all-the-icons-nerd-fonts-test-valid-icons
                  all-the-icons-nerd-fonts-test-missing-icons
                  all-the-icons-nerd-fonts-test-skipped-icons
                  all-the-icons-nerd-fonts-test-unknown-family
                  all-the-icons-nerd-fonts-test-entirely-unbound))
               (all-the-icons-nerd-fonts-test-valid-icons
                '((ok all-the-icons-nerd-fa "github")))
               (all-the-icons-nerd-fonts-test-missing-icons
                '((bad all-the-icons-nerd-fa
                       "definitely-missing")))
               (all-the-icons-nerd-fonts-test-skipped-icons
                '((web all-the-icons--web-mode-icon
                       "anything")))
               (all-the-icons-nerd-fonts-test-unknown-family
                '((bad all-the-icons-unknown "anything")))
               warnings)
           (cl-letf
               (((symbol-function 'display-warning)
                 (lambda (type message &rest arguments)
                   (push
                    (list type message arguments)
                    warnings))))
             (all-the-icons-nerd-fonts--check-configs)
             (nreverse warnings))))"##,
        expect![[
            r#"OK ((all-the-icons-nerd-fonts "Missing icon=definitely-missing from family=nerd-fa in var=all-the-icons-nerd-fonts-test-missing-icons" nil) (all-the-icons-nerd-fonts "Could not find data-alist=all-the-icons-data/unknown-alist from var=all-the-icons-nerd-fonts-test-unknown-family" nil) (all-the-icons-nerd-fonts "all-the-icons override variable not bound: all-the-icons-nerd-fonts-test-entirely-unbound" nil))"#
        ]],
    )
}

pub(super) fn prefer_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        readme_preference_migrates_real_use_site_alists_and_their_rendered_icons(),
        preference_is_idempotent_after_it_has_rewritten_real_associations(),
        advice_configuration_changes_direct_calls_but_not_alist_migration(),
        config_checker_distinguishes_valid_missing_skipped_unknown_and_unbound_sites(),
    ]
}
