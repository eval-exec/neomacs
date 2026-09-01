use expect_test::expect;

use super::ParityBatchCase;

fn australia_holidays_national_calendar_for_leap_year_2024_matches_exact_dates_and_labels()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_national_calendar_for_leap_year_2024_matches_exact_dates_and_labels",
        r##"(australia-holidays-test-year
                          australia-holidays
                          2024)"##,
        expect![[
            r#"OK (((1 1 2024) "New Year") ((1 26 2024) "Australia Day") ((3 29 2024) "Good Friday") ((4 1 2024) "Easter Monday") ((4 25 2024) "ANZAC Day") ((12 25 2024) "Christmas Day"))"#
        ]],
    )
}

fn australia_holidays_national_calendar_for_2025_matches_exact_dates_and_labels() -> ParityBatchCase
{
    ParityBatchCase::value(
        "australia_holidays_national_calendar_for_2025_matches_exact_dates_and_labels",
        r##"(australia-holidays-test-year
                          australia-holidays
                          2025)"##,
        expect![[
            r#"OK (((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((4 18 2025) "Good Friday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((12 25 2025) "Christmas Day"))"#
        ]],
    )
}

fn australia_holidays_national_calendar_for_2026_keeps_weekend_anzac_on_fixed_date()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_national_calendar_for_2026_keeps_weekend_anzac_on_fixed_date",
        r##"(australia-holidays-test-year
                          australia-holidays
                          2026)"##,
        expect![[
            r#"OK (((1 1 2026) "New Year") ((1 26 2026) "Australia Day") ((4 3 2026) "Good Friday") ((4 6 2026) "Easter Monday") ((4 25 2026) "ANZAC Day") ((12 25 2026) "Christmas Day"))"#
        ]],
    )
}

fn australia_holidays_cross_year_range_orders_december_and_january_holidays_chronologically()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_cross_year_range_orders_december_and_january_holidays_chronologically",
        r##"(australia-holidays-test-between
                          australia-holidays
                          '(12 20 2024)
                          '(2 2 2025))"##,
        expect![[
            r#"OK (((12 25 2024) "Christmas Day") ((1 1 2025) "New Year") ((1 26 2025) "Australia Day"))"#
        ]],
    )
}

fn australia_holidays_january_label_customization_is_read_at_each_calendar_evaluation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_january_label_customization_is_read_at_each_calendar_evaluation",
        r##"(let ((australia-holidays-include-january-26
                                t))
                           (let ((australia-holidays-january-26-label
                                  "Invasion Day"))
                             (let ((first
                                    (australia-holidays-test-on-date
                                     australia-holidays
                                     '(1 26 2025))))
                               (setq australia-holidays-january-26-label
                                     "Survival Day")
                               (let ((second
                                      (australia-holidays-test-on-date
                                       australia-holidays
                                       '(1 26 2025))))
                                 (setq australia-holidays-january-26-label
                                       "")
                                 (let ((empty-label
                                        (australia-holidays-test-on-date
                                         australia-holidays
                                         '(1 26 2025))))
                                   (list
                                    first
                                    second
                                    empty-label))))))"##,
        expect![[r#"OK (("Invasion Day") ("Survival Day") (""))"#]],
    )
}

fn australia_holidays_january_include_option_accepts_all_truthy_values_and_rejects_nil()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_january_include_option_accepts_all_truthy_values_and_rejects_nil",
        r##"(mapcar
                          (lambda (setting)
                            (let ((australia-holidays-include-january-26
                                   setting)
                                  (australia-holidays-january-26-label
                                   "January 26"))
                              (list
                               setting
                               (australia-holidays-test-on-date
                                australia-holidays
                                '(1 26 2028)))))
                          '(nil
                            t
                            :yes
                            0
                            ""
                            (enabled)))"##,
        expect![[
            r#"OK ((nil nil) (t ("January 26")) (:yes ("January 26")) (0 ("January 26")) ("" ("January 26")) ((enabled) ("January 26")))"#
        ]],
    )
}

fn australia_holidays_calendar_check_distinguishes_holidays_and_ordinary_dates() -> ParityBatchCase
{
    ParityBatchCase::value(
        "australia_holidays_calendar_check_distinguishes_holidays_and_ordinary_dates",
        r##"(mapcar
                          (lambda (date)
                            (list
                             date
                             (australia-holidays-test-on-date
                              australia-holidays
                              date)))
                          '((1 1 2025)
                            (1 2 2025)
                            (1 26 2025)
                            (4 18 2025)
                            (4 21 2025)
                            (4 25 2025)
                            (12 25 2025)
                            (12 26 2025)))"##,
        expect![[
            r#"OK (((1 1 2025) ("New Year")) ((1 2 2025) nil) ((1 26 2025) ("Australia Day")) ((4 18 2025) ("Good Friday")) ((4 21 2025) ("Easter Monday")) ((4 25 2025) ("ANZAC Day")) ((12 25 2025) ("Christmas Day")) ((12 26 2025) nil))"#
        ]],
    )
}

fn australia_holidays_fixed_rules_do_not_synthesize_weekday_substitute_holidays() -> ParityBatchCase
{
    ParityBatchCase::value(
        "australia_holidays_fixed_rules_do_not_synthesize_weekday_substitute_holidays",
        r##"(mapcar
                          (lambda (date)
                            (list
                             date
                             (calendar-day-of-week date)
                             (australia-holidays-test-on-date
                              australia-holidays
                              date)))
                          '((4 25 2021)
                            (4 26 2021)
                            (12 25 2021)
                            (12 27 2021)
                            (1 1 2022)
                            (1 3 2022)
                            (12 25 2022)
                            (12 27 2022)))"##,
        expect![[
            r#"OK (((4 25 2021) 0 ("ANZAC Day")) ((4 26 2021) 1 nil) ((12 25 2021) 6 ("Christmas Day")) ((12 27 2021) 1 nil) ((1 1 2022) 6 ("New Year")) ((1 3 2022) 1 nil) ((12 25 2022) 0 ("Christmas Day")) ((12 27 2022) 2 nil))"#
        ]],
    )
}

fn australia_holidays_easter_calculations_match_across_wide_early_and_late_date_sample()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_easter_calculations_match_across_wide_early_and_late_date_sample",
        r##"(mapcar
                          (lambda (year)
                            (let ((holidays
                                   (australia-holidays-test-year
                                    australia-holidays
                                    year)))
                              (list
                               year
                               (seq-filter
                                (lambda (holiday)
                                  (member
                                   (cadr holiday)
                                   '("Good Friday"
                                     "Easter Monday")))
                                holidays))))
                          '(1999
                            2000
                            2008
                            2011
                            2019
                            2024
                            2025
                            2038
                            2099
                            2100))"##,
        expect![[
            r#"OK ((1999 (((4 2 1999) "Good Friday") ((4 5 1999) "Easter Monday"))) (2000 (((4 21 2000) "Good Friday") ((4 24 2000) "Easter Monday"))) (2008 (((3 21 2008) "Good Friday") ((3 24 2008) "Easter Monday"))) (2011 (((4 22 2011) "Good Friday") ((4 25 2011) "Easter Monday"))) (2019 (((4 19 2019) "Good Friday") ((4 22 2019) "Easter Monday"))) (2024 (((3 29 2024) "Good Friday") ((4 1 2024) "Easter Monday"))) (2025 (((4 18 2025) "Good Friday") ((4 21 2025) "Easter Monday"))) (2038 (((4 23 2038) "Good Friday") ((4 26 2038) "Easter Monday"))) (2099 (((4 10 2099) "Good Friday") ((4 13 2099) "Easter Monday"))) (2100 (((3 26 2100) "Good Friday") ((3 29 2100) "Easter Monday"))))"#
        ]],
    )
}

fn australia_holidays_three_month_calendar_window_handles_year_boundary_visibility()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_three_month_calendar_window_handles_year_boundary_visibility",
        r##"(let ((calendar-holidays
                                australia-holidays)
                               (displayed-month 12)
                               (displayed-year 2025)
                               (calendar-total-months 3))
                           (calendar-holiday-list))"##,
        expect!["OK nil"],
    )
}

fn australia_holidays_non_string_january_labels_flow_through_rule_evaluation_and_rendering_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_non_string_january_labels_flow_through_rule_evaluation_and_rendering_errors",
        r##"(mapcar
                          (lambda (label)
                            (let ((australia-holidays-january-26-label
                                   label)
                                  (australia-holidays-include-january-26
                                   t))
                              (list
                               label
                               (australia-holidays-test-on-date
                                australia-holidays
                                '(1 26 2025))
                               (australia-holidays-test-error
                                (lambda ()
                                  (mapconcat
                                   (lambda (holiday)
                                     (concat
                                      (calendar-date-string
                                       (car holiday))
                                      ": "
                                      (cadr holiday)))
                                   (australia-holidays-test-year
                                    australia-holidays
                                    2025)
                                   "\n"))))))
                          '(nil
                            symbol
                            17
                            ("list")
                            "Valid"))"##,
        expect![[
            r#"OK ((nil (nil) (:ok "Wednesday, January 1, 2025: New Year\nSunday, January 26, 2025: \nFriday, April 18, 2025: Good Friday\nMonday, April 21, 2025: Easter Monday\nFriday, April 25, 2025: ANZAC Day\nThursday, December 25, 2025: Christmas Day")) (symbol (symbol) (:signal wrong-type-argument (sequencep symbol))) (17 (17) (:signal wrong-type-argument (sequencep 17))) (#1=("list") (#1#) (:signal wrong-type-argument (characterp "list"))) ("Valid" ("Valid") (:ok "Wednesday, January 1, 2025: New Year\nSunday, January 26, 2025: Valid\nFriday, April 18, 2025: Good Friday\nMonday, April 21, 2025: Easter Monday\nFriday, April 25, 2025: ANZAC Day\nThursday, December 25, 2025: Christmas Day")))"#
        ]],
    )
}

pub(super) fn national_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        australia_holidays_national_calendar_for_leap_year_2024_matches_exact_dates_and_labels(),
        australia_holidays_national_calendar_for_2025_matches_exact_dates_and_labels(),
        australia_holidays_national_calendar_for_2026_keeps_weekend_anzac_on_fixed_date(),
        australia_holidays_cross_year_range_orders_december_and_january_holidays_chronologically(),
        australia_holidays_january_label_customization_is_read_at_each_calendar_evaluation(),
        australia_holidays_january_include_option_accepts_all_truthy_values_and_rejects_nil(),
        australia_holidays_calendar_check_distinguishes_holidays_and_ordinary_dates(),
        australia_holidays_fixed_rules_do_not_synthesize_weekday_substitute_holidays(),
        australia_holidays_easter_calculations_match_across_wide_early_and_late_date_sample(),
        australia_holidays_three_month_calendar_window_handles_year_boundary_visibility(),
        australia_holidays_non_string_january_labels_flow_through_rule_evaluation_and_rendering_errors(),
    ]
}
