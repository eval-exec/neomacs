use expect_test::expect;

use super::ParityBatchCase;

fn australia_holidays_act_2025_calendar_surfaces_symbolic_national_list_engine_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_act_2025_calendar_surfaces_symbolic_national_list_engine_failure",
        r##"(list
                          (car
                           australia-holidays-for-act)
                          (eval
                           (car
                            australia-holidays-for-act)
                           t)
                          (australia-holidays-test-error
                           (lambda ()
                             (australia-holidays-test-year
                              australia-holidays-for-act
                              2025))))"##,
        expect![[
            r#"OK (australia-holidays ((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day")) (:signal wrong-type-argument (listp holiday-fixed)))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_nsw_2025_calendar_matches_practical_full_year_schedule() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_nsw_2025_calendar_matches_practical_full_year_schedule",
        r##"(australia-holidays-test-year
                          australia-holidays-for-nsw
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((4 18 2025) "Good Friday") ((4 19 2025) "Easter Saturday") ((4 20 2025) "Easter Sunday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((4 25 2025) "ANZAC Day") ((6 9 2025) "King's Birthday") ((10 6 2025) "Labour Day") ((12 25 2025) "Christmas Day") ((12 26 2025) "Boxing Day"))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_nt_2025_calendar_matches_practical_full_year_schedule() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_nt_2025_calendar_matches_practical_full_year_schedule",
        r##"(australia-holidays-test-year
                          australia-holidays-for-nt
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((4 18 2025) "Good Friday") ((4 19 2025) "Easter Saturday") ((4 20 2025) "Easter Sunday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((5 5 2025) "May Day") ((6 9 2025) "King's Birthday") ((8 4 2025) "Picnic Day") ((12 24 2025) "Christmas Eve") ((12 25 2025) "Christmas Day") ((12 26 2025) "Boxing Day") ((12 31 2025) "New Year's Eve"))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_qld_2025_calendar_matches_practical_full_year_schedule() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_qld_2025_calendar_matches_practical_full_year_schedule",
        r##"(australia-holidays-test-year
                          australia-holidays-for-qld
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((4 18 2025) "Good Friday") ((4 19 2025) "The Day After Good Friday") ((4 20 2025) "Easter Sunday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((5 5 2025) "Labour Day") ((8 13 2025) "Royal Queensland Show") ((10 6 2025) "King's Birthday") ((12 24 2025) "Christmas Eve") ((12 25 2025) "Christmas Day") ((12 26 2025) "Boxing Day"))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_sa_2025_calendar_matches_practical_full_year_schedule() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_sa_2025_calendar_matches_practical_full_year_schedule",
        r##"(australia-holidays-test-year
                          australia-holidays-for-sa
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((3 10 2025) "Adelaide Cup Day") ((4 18 2025) "Good Friday") ((4 19 2025) "Easter Saturday") ((4 20 2025) "Easter Sunday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((6 9 2025) "King's Birthday") ((10 6 2025) "Labour Day") ((12 24 2025) "Christmas Eve") ((12 25 2025) "Christmas Day") ((12 26 2025) "Proclamation Day") ((12 31 2025) "New Year's Eve"))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_tas_2025_calendar_matches_practical_full_year_schedule() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_tas_2025_calendar_matches_practical_full_year_schedule",
        r##"(australia-holidays-test-year
                          australia-holidays-for-tas
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((2 10 2025) "Royal Hobart Regatta") ((3 10 2025) "Eight Hours Day") ((4 18 2025) "Good Friday") ((4 21 2025) "Easter Monday") ((4 22 2025) "Easter Tuesday") ((4 25 2025) "ANZAC Day") ((6 9 2025) "King's Birthday") ((11 3 2025) "Recreation Day") ((12 25 2025) "Christmas Day") ((12 26 2025) "Boxing Day"))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_vic_2025_calendar_matches_practical_full_year_schedule() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_vic_2025_calendar_matches_practical_full_year_schedule",
        r##"(australia-holidays-test-year
                          australia-holidays-for-vic
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((3 10 2025) "Labour Day") ((4 18 2025) "Good Friday") ((4 19 2025) "Saturday Before Easter Sunday") ((4 20 2025) "Easter Sunday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((6 9 2025) "King's Birthday") ((9 26 2025) "Friday before AFL Grand Final") ((11 4 2025) "Melbourne Cup") ((12 25 2025) "Christmas Day") ((12 26 2025) "Boxing Day"))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_wa_2025_calendar_matches_practical_full_year_schedule() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_wa_2025_calendar_matches_practical_full_year_schedule",
        r##"(australia-holidays-test-year
                          australia-holidays-for-wa
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((3 3 2025) "Labour Day") ((4 18 2025) "Good Friday") ((4 20 2025) "Easter Sunday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((6 2 2025) "Western Australia Day") ((12 25 2025) "Christmas Day") ((12 26 2025) "Boxing Day"))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_all_regional_calendars_have_exact_counts_and_boundaries_across_three_years()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_all_regional_calendars_have_exact_counts_and_boundaries_across_three_years",
        r##"(mapcar
                          (lambda (symbol)
                            (list
                             symbol
                             (mapcar
                              (lambda (year)
                                (list
                                 year
                                 (australia-holidays-test-error
                                  (lambda ()
                                    (let ((holidays
                                           (australia-holidays-test-year-by-symbol
                                            symbol
                                            year)))
                                      (list
                                       (length holidays)
                                       (car holidays)
                                       (car
                                        (last holidays))))))))
                              '(2024 2025 2026))))
                          '(australia-holidays-for-act
                            australia-holidays-for-nsw
                            australia-holidays-for-nt
                            australia-holidays-for-qld
                            australia-holidays-for-sa
                            australia-holidays-for-tas
                            australia-holidays-for-vic
                            australia-holidays-for-wa))"##,
        expect![[
            r#"OK ((australia-holidays-for-act ((2024 (:signal wrong-type-argument (listp holiday-fixed))) (2025 (:signal wrong-type-argument (listp holiday-fixed))) (2026 (:signal wrong-type-argument (listp holiday-fixed))))) (australia-holidays-for-nsw ((2024 (:ok (12 ((1 1 2024) "New Year") ((12 26 2024) "Boxing Day")))) (2025 (:ok (12 ((1 1 2025) "New Year") ((12 26 2025) "Boxing Day")))) (2026 (:ok (12 ((1 1 2026) "New Year") ((12 26 2026) "Boxing Day")))))) (australia-holidays-for-nt ((2024 (:ok (14 ((1 1 2024) "New Year") ((12 31 2024) "New Year's Eve")))) (2025 (:ok (14 ((1 1 2025) "New Year") ((12 31 2025) "New Year's Eve")))) (2026 (:ok (14 ((1 1 2026) "New Year") ((12 31 2026) "New Year's Eve")))))) (australia-holidays-for-qld ((2024 (:ok (13 ((1 1 2024) "New Year") ((12 26 2024) "Boxing Day")))) (2025 (:ok (13 ((1 1 2025) "New Year") ((12 26 2025) "Boxing Day")))) (2026 (:ok (13 ((1 1 2026) "New Year") ((12 26 2026) "Boxing Day")))))) (australia-holidays-for-sa ((2024 (:ok (14 ((1 1 2024) "New Year") ((12 31 2024) "New Year's Eve")))) (2025 (:ok (14 ((1 1 2025) "New Year") ((12 31 2025) "New Year's Eve")))) (2026 (:ok (14 ((1 1 2026) "New Year") ((12 31 2026) "New Year's Eve")))))) (australia-holidays-for-tas ((2024 (:ok (12 ((1 1 2024) "New Year") ((12 26 2024) "Boxing Day")))) (2025 (:ok (12 ((1 1 2025) "New Year") ((12 26 2025) "Boxing Day")))) (2026 (:ok (12 ((1 1 2026) "New Year") ((12 26 2026) "Boxing Day")))))) (australia-holidays-for-vic ((2024 (:ok (13 ((1 1 2024) "New Year") ((12 26 2024) "Boxing Day")))) (2025 (:ok (13 ((1 1 2025) "New Year") ((12 26 2025) "Boxing Day")))) (2026 (:ok (13 ((1 1 2026) "New Year") ((12 26 2026) "Boxing Day")))))) (australia-holidays-for-wa ((2024 (:ok (10 ((1 1 2024) "New Year") ((12 26 2024) "Boxing Day")))) (2025 (:ok (10 ((1 1 2025) "New Year") ((12 26 2025) "Boxing Day")))) (2026 (:ok (10 ((1 1 2026) "New Year") ((12 26 2026) "Boxing Day")))))))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_nsw_keeps_duplicate_anzac_rule_and_calendar_results() -> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_nsw_keeps_duplicate_anzac_rule_and_calendar_results",
        r##"(list
                          (seq-filter
                           (lambda (rule)
                             (equal
                              rule
                              '(holiday-fixed
                                4
                                25
                                "ANZAC Day")))
                           australia-holidays-for-nsw)
                          (australia-holidays-test-on-date
                           australia-holidays-for-nsw
                           '(4 25 2025))
                          (seq-filter
                           (lambda (holiday)
                             (equal
                              (car holiday)
                              '(4 25 2025)))
                           (australia-holidays-test-year
                            australia-holidays-for-nsw
                            2025)))"##,
        expect![[
            r#"OK (((holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 4 25 "ANZAC Day")) ("ANZAC Day" "ANZAC Day") (((4 25 2025) "ANZAC Day") ((4 25 2025) "ANZAC Day")))"#
        ]],
    )
}

fn australia_holidays_act_tracks_reassigned_national_list_while_copied_state_lists_do_not()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_act_tracks_reassigned_national_list_while_copied_state_lists_do_not",
        r##"(let ((original-national
                                (copy-tree
                                 australia-holidays))
                               (original-nsw-prefix
                                (seq-take
                                 australia-holidays-for-nsw
                                 (length australia-holidays)))
                               (act-before
                                (eval
                                 (car
                                  australia-holidays-for-act)
                                 t))
                               (act-error-before
                                (australia-holidays-test-error
                                 (lambda ()
                                   (australia-holidays-test-year
                                    australia-holidays-for-act
                                    2025)))))
                           (setq australia-holidays
                                 '((holiday-fixed
                                    7
                                    7
                                    "Replacement National")))
                           (list
                            original-national
                            act-before
                            act-error-before
                            australia-holidays
                            (eval
                             (car
                              australia-holidays-for-act)
                             t)
                            (australia-holidays-test-error
                             (lambda ()
                               (australia-holidays-test-year
                                australia-holidays-for-act
                                2025)))
                            original-nsw-prefix
                            (seq-take
                             australia-holidays-for-nsw
                             (length original-national))
                            (australia-holidays-test-on-date
                             australia-holidays-for-nsw
                             '(1 1 2025))
                            (australia-holidays-test-on-date
                             australia-holidays-for-nsw
                             '(7 7 2025))))"##,
        expect![[
            r#"OK (((holiday-fixed 1 1 "New Year") (if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) (holiday-easter-etc -2 "Good Friday") (holiday-easter-etc 1 "Easter Monday") (holiday-fixed 4 25 "ANZAC Day") (holiday-fixed 12 25 "Christmas Day")) (#2=(holiday-fixed 1 1 "New Year") #3=(if australia-holidays-include-january-26 (holiday-fixed 1 26 australia-holidays-january-26-label)) #4=(holiday-easter-etc -2 "Good Friday") #5=(holiday-easter-etc 1 "Easter Monday") #6=(holiday-fixed 4 25 "ANZAC Day") #7=(holiday-fixed 12 25 "Christmas Day")) (:signal wrong-type-argument (listp holiday-fixed)) #1=((holiday-fixed 7 7 "Replacement National")) #1# (:signal wrong-type-argument (listp holiday-fixed)) (#2# #3# #4# #5# #6# #7#) (#2# #3# #4# #5# #6# #7#) ("New Year") nil)"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_mutating_shared_national_rule_object_updates_every_copied_state_list()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_mutating_shared_national_rule_object_updates_every_copied_state_list",
        r##"(let ((base-rule
                                (car australia-holidays)))
                           (setcar
                            (cdddr base-rule)
                            "Renamed New Year")
                           (mapcar
                            (lambda (symbol)
                              (list
                               symbol
                               (eq
                                (car
                                 (symbol-value symbol))
                                base-rule)
                               (australia-holidays-test-on-date
                                (symbol-value symbol)
                                '(1 1 2025))))
                            '(australia-holidays
                              australia-holidays-for-nsw
                              australia-holidays-for-nt
                              australia-holidays-for-qld
                              australia-holidays-for-sa
                              australia-holidays-for-tas
                              australia-holidays-for-vic
                              australia-holidays-for-wa)))"##,
        expect![[
            r#"OK ((australia-holidays t ("Renamed New Year")) (australia-holidays-for-nsw t ("Renamed New Year")) (australia-holidays-for-nt t ("Renamed New Year")) (australia-holidays-for-qld t ("Renamed New Year")) (australia-holidays-for-sa t ("Renamed New Year")) (australia-holidays-for-tas t ("Renamed New Year")) (australia-holidays-for-vic t ("Renamed New Year")) (australia-holidays-for-wa t ("Renamed New Year")))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_january_customization_propagates_through_every_regional_calendar()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_january_customization_propagates_through_every_regional_calendar",
        r##"(let ((symbols
                                '(australia-holidays-for-act
                                  australia-holidays-for-nsw
                                  australia-holidays-for-nt
                                  australia-holidays-for-qld
                                  australia-holidays-for-sa
                                  australia-holidays-for-tas
                                  australia-holidays-for-vic
                                  australia-holidays-for-wa))
                               (australia-holidays-january-26-label
                                "Community Day"))
                           (let ((australia-holidays-include-january-26
                                  t))
                             (let ((included
                                    (mapcar
                                     (lambda (symbol)
                                       (list
                                        symbol
                                        (australia-holidays-test-error
                                         (lambda ()
                                           (australia-holidays-test-on-date
                                            (symbol-value symbol)
                                            '(1 26 2025))))))
                                     symbols)))
                               (setq australia-holidays-include-january-26
                                     nil)
                               (list
                                included
                                (mapcar
                                 (lambda (symbol)
                                   (list
                                    symbol
                                    (australia-holidays-test-error
                                     (lambda ()
                                       (australia-holidays-test-on-date
                                        (symbol-value symbol)
                                        '(1 26 2025))))))
                                 symbols)))))"##,
        expect![[
            r#"OK (((australia-holidays-for-act (:signal wrong-type-argument (listp if))) (australia-holidays-for-nsw (:ok ("Community Day"))) (australia-holidays-for-nt (:ok ("Community Day"))) (australia-holidays-for-qld (:ok ("Community Day"))) (australia-holidays-for-sa (:ok ("Community Day"))) (australia-holidays-for-tas (:ok ("Community Day"))) (australia-holidays-for-vic (:ok ("Community Day"))) (australia-holidays-for-wa (:ok ("Community Day")))) ((australia-holidays-for-act (:signal wrong-type-argument (listp if))) (australia-holidays-for-nsw (:ok nil)) (australia-holidays-for-nt (:ok nil)) (australia-holidays-for-qld (:ok nil)) (australia-holidays-for-sa (:ok nil)) (australia-holidays-for-tas (:ok nil)) (australia-holidays-for-vic (:ok nil)) (australia-holidays-for-wa (:ok nil))))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_qld_show_and_victorian_moving_holidays_match_multi_year_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_qld_show_and_victorian_moving_holidays_match_multi_year_rules",
        r##"(mapcar
                          (lambda (year)
                            (list
                             year
                             (seq-filter
                              (lambda (holiday)
                                (member
                                 (cadr holiday)
                                 '("Royal Queensland Show"
                                   "King's Birthday")))
                              (australia-holidays-test-year
                               australia-holidays-for-qld
                               year))
                             (seq-filter
                              (lambda (holiday)
                                (member
                                 (cadr holiday)
                                 '("Labour Day"
                                   "Melbourne Cup"
                                   "Friday before AFL Grand Final")))
                              (australia-holidays-test-year
                               australia-holidays-for-vic
                               year))))
                          '(2023
                            2024
                            2025
                            2026
                            2027
                            2028))"##,
        expect![[
            r#"OK ((2023 (((8 9 2023) "Royal Queensland Show") ((10 2 2023) "King's Birthday")) (((3 13 2023) "Labour Day") ((9 29 2023) "Friday before AFL Grand Final") ((11 7 2023) "Melbourne Cup"))) (2024 (((8 14 2024) "Royal Queensland Show") ((10 7 2024) "King's Birthday")) (((3 11 2024) "Labour Day") ((9 27 2024) "Friday before AFL Grand Final") ((11 5 2024) "Melbourne Cup"))) (2025 (((8 13 2025) "Royal Queensland Show") ((10 6 2025) "King's Birthday")) (((3 10 2025) "Labour Day") ((9 26 2025) "Friday before AFL Grand Final") ((11 4 2025) "Melbourne Cup"))) (2026 (((8 12 2026) "Royal Queensland Show") ((10 5 2026) "King's Birthday")) (((3 9 2026) "Labour Day") ((9 25 2026) "Friday before AFL Grand Final") ((11 3 2026) "Melbourne Cup"))) (2027 (((8 11 2027) "Royal Queensland Show") ((10 4 2027) "King's Birthday")) (((3 8 2027) "Labour Day") ((9 24 2027) "Friday before AFL Grand Final") ((11 2 2027) "Melbourne Cup"))) (2028 (((8 9 2028) "Royal Queensland Show") ((10 2 2028) "King's Birthday")) (((3 13 2028) "Labour Day") ((9 29 2028) "Friday before AFL Grand Final") ((11 7 2028) "Melbourne Cup"))))"#
        ]],
    )
    .fresh_process()
}

fn australia_holidays_act_tasmania_and_wa_special_moving_days_match_multi_year_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_act_tasmania_and_wa_special_moving_days_match_multi_year_rules",
        r##"(mapcar
                          (lambda (year)
                            (list
                             year
                             (australia-holidays-test-error
                              (lambda ()
                                (seq-filter
                                 (lambda (holiday)
                                   (member
                                    (cadr holiday)
                                    '("Canberra Day"
                                      "Reconciliation Day")))
                                 (australia-holidays-test-year
                                  australia-holidays-for-act
                                  year))))
                             (seq-filter
                              (lambda (holiday)
                                (member
                                 (cadr holiday)
                                 '("Royal Hobart Regatta"
                                   "Eight Hours Day"
                                   "Easter Tuesday"
                                   "Recreation Day")))
                              (australia-holidays-test-year
                               australia-holidays-for-tas
                               year))
                             (seq-filter
                              (lambda (holiday)
                                (member
                                 (cadr holiday)
                                 '("Labour Day"
                                   "Western Australia Day")))
                              (australia-holidays-test-year
                               australia-holidays-for-wa
                               year))))
                          '(2024
                            2025
                            2026
                            2027))"##,
        expect![[
            r#"OK ((2024 (:signal wrong-type-argument (listp holiday-fixed)) (((2 12 2024) "Royal Hobart Regatta") ((3 11 2024) "Eight Hours Day") ((4 2 2024) "Easter Tuesday") ((11 4 2024) "Recreation Day")) (((3 4 2024) "Labour Day") ((6 3 2024) "Western Australia Day"))) (2025 (:signal wrong-type-argument (listp holiday-fixed)) (((2 10 2025) "Royal Hobart Regatta") ((3 10 2025) "Eight Hours Day") ((4 22 2025) "Easter Tuesday") ((11 3 2025) "Recreation Day")) (((3 3 2025) "Labour Day") ((6 2 2025) "Western Australia Day"))) (2026 (:signal wrong-type-argument (listp holiday-fixed)) (((2 9 2026) "Royal Hobart Regatta") ((3 9 2026) "Eight Hours Day") ((4 7 2026) "Easter Tuesday") ((11 2 2026) "Recreation Day")) (((3 2 2026) "Labour Day") ((6 1 2026) "Western Australia Day"))) (2027 (:signal wrong-type-argument (listp holiday-fixed)) (((2 8 2027) "Royal Hobart Regatta") ((3 8 2027) "Eight Hours Day") ((3 30 2027) "Easter Tuesday") ((11 1 2027) "Recreation Day")) (((3 1 2027) "Labour Day") ((6 7 2027) "Western Australia Day"))))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn states_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        australia_holidays_act_2025_calendar_surfaces_symbolic_national_list_engine_failure(),
        australia_holidays_nsw_2025_calendar_matches_practical_full_year_schedule(),
        australia_holidays_nt_2025_calendar_matches_practical_full_year_schedule(),
        australia_holidays_qld_2025_calendar_matches_practical_full_year_schedule(),
        australia_holidays_sa_2025_calendar_matches_practical_full_year_schedule(),
        australia_holidays_tas_2025_calendar_matches_practical_full_year_schedule(),
        australia_holidays_vic_2025_calendar_matches_practical_full_year_schedule(),
        australia_holidays_wa_2025_calendar_matches_practical_full_year_schedule(),
        australia_holidays_all_regional_calendars_have_exact_counts_and_boundaries_across_three_years(),
        australia_holidays_nsw_keeps_duplicate_anzac_rule_and_calendar_results(),
        australia_holidays_act_tracks_reassigned_national_list_while_copied_state_lists_do_not(),
        australia_holidays_mutating_shared_national_rule_object_updates_every_copied_state_list(),
        australia_holidays_january_customization_propagates_through_every_regional_calendar(),
        australia_holidays_qld_show_and_victorian_moving_holidays_match_multi_year_rules(),
        australia_holidays_act_tasmania_and_wa_special_moving_days_match_multi_year_rules(),
    ]
}
