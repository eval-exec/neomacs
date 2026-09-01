use expect_test::expect;

use super::ParityBatchCase;

fn australia_holidays_real_list_holidays_renders_complete_national_2025_buffer() -> ParityBatchCase
{
    ParityBatchCase::value(
        "australia_holidays_real_list_holidays_renders_complete_national_2025_buffer",
        r##"(let ((holiday-buffer
                                " *Australia National Holidays*"))
                           (unwind-protect
                               (let ((result
                                      (list-holidays
                                       2025
                                       2025
                                       australia-holidays
                                       "Australian National Holidays")))
                                 (with-current-buffer
                                     holiday-buffer
                                   (list
                                    result
                                    (buffer-string)
                                    mode-line-format
                                    buffer-read-only
                                    major-mode
                                    (point-min)
                                    (point-max))))
                             (when
                                 (get-buffer holiday-buffer)
                               (kill-buffer holiday-buffer))))"##,
        expect![[
            r#"OK ("Computing holidays...done" "Wednesday, January 1, 2025: New Year\nSunday, January 26, 2025: Australia Day\nFriday, April 18, 2025: Good Friday\nMonday, April 21, 2025: Easter Monday\nFriday, April 25, 2025: ANZAC Day\nThursday, December 25, 2025: Christmas Day" "---------------------Australian National Holidays for 2025----------------------" t special-mode 1 228)"#
        ]],
    )
}

fn australia_holidays_real_list_holidays_renders_victorian_multi_year_schedule_and_label()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_real_list_holidays_renders_victorian_multi_year_schedule_and_label",
        r##"(let ((holiday-buffer
                                " *Victoria Holidays*"))
                           (unwind-protect
                               (let ((result
                                      (list-holidays
                                       2024
                                       2025
                                       australia-holidays-for-vic
                                       "Victorian Public Holidays")))
                                 (with-current-buffer
                                     holiday-buffer
                                   (list
                                    result
                                    (buffer-string)
                                    mode-line-format
                                    buffer-read-only
                                    major-mode)))
                             (when
                                 (get-buffer holiday-buffer)
                               (kill-buffer holiday-buffer))))"##,
        expect![[
            r#"OK ("Computing holidays...done" "Monday, January 1, 2024: New Year\nFriday, January 26, 2024: Australia Day\nMonday, March 11, 2024: Labour Day\nFriday, March 29, 2024: Good Friday\nSaturday, March 30, 2024: Saturday Before Easter Sunday\nSunday, March 31, 2024: Easter Sunday\nMonday, April 1, 2024: Easter Monday\nThursday, April 25, 2024: ANZAC Day\nMonday, June 10, 2024: King's Birthday\nFriday, September 27, 2024: Friday before AFL Grand Final\nTuesday, November 5, 2024: Melbourne Cup\nWednesday, December 25, 2024: Christmas Day\nThursday, December 26, 2024: Boxing Day\nWednesday, January 1, 2025: New Year\nSunday, January 26, 2025: Australia Day\nMonday, March 10, 2025: Labour Day\nFriday, April 18, 2025: Good Friday\nSaturday, April 19, 2025: Saturday Before Easter Sunday\nSunday, April 20, 2025: Easter Sunday\nMonday, April 21, 2025: Easter Monday\nFriday, April 25, 2025: ANZAC Day\nMonday, June 9, 2025: King's Birthday\nFriday, September 26, 2025: Friday before AFL Grand Final\nTuesday, November 4, 2025: Melbourne Cup\nThursday, December 25, 2025: Christmas Day\nFriday, December 26, 2025: Boxing Day" "--------------------Victorian Public Holidays for 2024-2025---------------------" t special-mode)"#
        ]],
    )
}

fn australia_holidays_custom_january_label_flows_into_real_rendered_holiday_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_custom_january_label_flows_into_real_rendered_holiday_buffer",
        r##"(let ((holiday-buffer
                                " *Customized Australia Holidays*")
                               (australia-holidays-january-26-label
                                "First Nations Survival Day")
                               (australia-holidays-include-january-26
                                t))
                           (unwind-protect
                               (progn
                                 (list-holidays
                                  2025
                                  2025
                                  australia-holidays
                                  "Customized Australia")
                                 (with-current-buffer
                                     holiday-buffer
                                   (list
                                    (buffer-string)
                                    (save-excursion
                                      (goto-char
                                       (point-min))
                                      (search-forward
                                       "First Nations Survival Day"
                                       nil
                                       t)))))
                             (when
                                 (get-buffer holiday-buffer)
                               (kill-buffer holiday-buffer))))"##,
        expect![[
            r#"OK ("Wednesday, January 1, 2025: New Year\nSunday, January 26, 2025: First Nations Survival Day\nFriday, April 18, 2025: Good Friday\nMonday, April 21, 2025: Easter Monday\nFriday, April 25, 2025: ANZAC Day\nThursday, December 25, 2025: Christmas Day" 90)"#
        ]],
    )
}

fn australia_holidays_can_append_to_existing_calendar_rules_without_losing_order_or_collisions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_can_append_to_existing_calendar_rules_without_losing_order_or_collisions",
        r##"(let* ((existing
                                 '((holiday-fixed
                                    1
                                    1
                                    "Personal New Year")
                                   (holiday-fixed
                                    7
                                    14
                                    "Personal Anniversary")))
                                (combined
                                 (append
                                  existing
                                  australia-holidays)))
                           (list
                            (australia-holidays-test-year
                             combined
                             2025)
                            (australia-holidays-test-on-date
                             combined
                             '(1 1 2025))
                            (australia-holidays-test-on-date
                             combined
                             '(7 14 2025))))"##,
        expect![[
            r#"OK ((((1 1 2025) "Personal New Year") ((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((4 18 2025) "Good Friday") ((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day") ((7 14 2025) "Personal Anniversary") ((12 25 2025) "Christmas Day")) ("New Year" "Personal New Year") ("Personal Anniversary"))"#
        ]],
    )
}

fn australia_holidays_calendar_list_holidays_builds_real_three_month_boundary_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_calendar_list_holidays_builds_real_three_month_boundary_buffer",
        r##"(let ((holiday-buffer
                                " *Australia Calendar Window*"))
                           (unwind-protect
                               (cl-progv
                                   '(calendar-holidays
                                     displayed-month
                                     displayed-year
                                     calendar-total-months)
                                   (list
                                    australia-holidays
                                    12
                                    2025
                                    3)
                                 (let ((result
                                        (calendar-list-holidays)))
                                   (with-current-buffer
                                       holiday-buffer
                                     (list
                                      result
                                      (buffer-string)
                                      mode-line-format
                                      buffer-read-only
                                      major-mode))))
                             (when
                                 (get-buffer holiday-buffer)
                               (kill-buffer holiday-buffer))))"##,
        expect![[
            r#"OK ((((12 25 2025) "Christmas Day") ((1 1 2026) "New Year") ((1 26 2026) "Australia Day")) "Thursday, December 25, 2025: Christmas Day\nThursday, January 1, 2026: New Year\nMonday, January 26, 2026: Australia Day" "--------------Notable Dates from November, 2025 to January, 2026%---------------" t special-mode)"#
        ]],
    )
}

fn australia_holidays_holiday_in_range_is_inclusive_for_single_day_and_cross_month_queries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_holiday_in_range_is_inclusive_for_single_day_and_cross_month_queries",
        r##"(list
                          (australia-holidays-test-between
                           australia-holidays
                           '(4 25 2025)
                           '(4 25 2025))
                          (australia-holidays-test-between
                           australia-holidays
                           '(4 19 2025)
                           '(4 25 2025))
                          (australia-holidays-test-between
                           australia-holidays
                           '(4 26 2025)
                           '(12 24 2025))
                          (australia-holidays-test-between
                           australia-holidays
                           '(12 25 2025)
                           '(1 1 2026)))"##,
        expect![[
            r#"OK ((((4 25 2025) "ANZAC Day")) (((4 21 2025) "Easter Monday") ((4 25 2025) "ANZAC Day")) nil (((12 25 2025) "Christmas Day") ((1 1 2026) "New Year")))"#
        ]],
    )
}

fn australia_holidays_every_region_keeps_christmas_related_rules_on_literal_weekend_dates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_every_region_keeps_christmas_related_rules_on_literal_weekend_dates",
        r##"(mapcar
                          (lambda (symbol)
                            (list
                             symbol
                             (mapcar
                              (lambda (date)
                                (list
                                 date
                                 (calendar-day-of-week date)
                                 (australia-holidays-test-error
                                  (lambda ()
                                    (australia-holidays-test-on-date
                                     (symbol-value symbol)
                                     date)))))
                              '((12 24 2021)
                                (12 25 2021)
                                (12 26 2021)
                                (12 27 2021)
                                (12 28 2021)
                                (12 31 2021)))))
                          '(australia-holidays-for-act
                            australia-holidays-for-nsw
                            australia-holidays-for-nt
                            australia-holidays-for-qld
                            australia-holidays-for-sa
                            australia-holidays-for-tas
                            australia-holidays-for-vic
                            australia-holidays-for-wa))"##,
        expect![[
            r#"OK ((australia-holidays-for-act ((#1=(12 24 2021) 5 (:signal wrong-type-argument (listp holiday-fixed))) (#2=(12 25 2021) 6 (:signal wrong-type-argument (listp holiday-fixed))) (#3=(12 26 2021) 0 (:signal wrong-type-argument (listp holiday-fixed))) (#4=(12 27 2021) 1 (:signal wrong-type-argument (listp holiday-fixed))) (#5=(12 28 2021) 2 (:signal wrong-type-argument (listp holiday-fixed))) (#6=(12 31 2021) 5 (:signal wrong-type-argument (listp holiday-fixed))))) (australia-holidays-for-nsw ((#1# 5 (:ok nil)) (#2# 6 (:ok ("Christmas Day"))) (#3# 0 (:ok ("Boxing Day"))) (#4# 1 (:ok nil)) (#5# 2 (:ok nil)) (#6# 5 (:ok nil)))) (australia-holidays-for-nt ((#1# 5 (:ok ("Christmas Eve"))) (#2# 6 (:ok ("Christmas Day"))) (#3# 0 (:ok ("Boxing Day"))) (#4# 1 (:ok nil)) (#5# 2 (:ok nil)) (#6# 5 (:ok ("New Year's Eve"))))) (australia-holidays-for-qld ((#1# 5 (:ok ("Christmas Eve"))) (#2# 6 (:ok ("Christmas Day"))) (#3# 0 (:ok ("Boxing Day"))) (#4# 1 (:ok nil)) (#5# 2 (:ok nil)) (#6# 5 (:ok nil)))) (australia-holidays-for-sa ((#1# 5 (:ok ("Christmas Eve"))) (#2# 6 (:ok ("Christmas Day"))) (#3# 0 (:ok ("Proclamation Day"))) (#4# 1 (:ok nil)) (#5# 2 (:ok nil)) (#6# 5 (:ok ("New Year's Eve"))))) (australia-holidays-for-tas ((#1# 5 (:ok nil)) (#2# 6 (:ok ("Christmas Day"))) (#3# 0 (:ok ("Boxing Day"))) (#4# 1 (:ok nil)) (#5# 2 (:ok nil)) (#6# 5 (:ok nil)))) (australia-holidays-for-vic ((#1# 5 (:ok nil)) (#2# 6 (:ok ("Christmas Day"))) (#3# 0 (:ok ("Boxing Day"))) (#4# 1 (:ok nil)) (#5# 2 (:ok nil)) (#6# 5 (:ok nil)))) (australia-holidays-for-wa ((#1# 5 (:ok nil)) (#2# 6 (:ok ("Christmas Day"))) (#3# 0 (:ok ("Boxing Day"))) (#4# 1 (:ok nil)) (#5# 2 (:ok nil)) (#6# 5 (:ok nil)))))"#
        ]],
    )
}

fn australia_holidays_bad_rule_isolated_by_calendar_engine_while_valid_package_rules_still_render()
-> ParityBatchCase {
    ParityBatchCase::value(
        "australia_holidays_bad_rule_isolated_by_calendar_engine_while_valid_package_rules_still_render",
        r##"(let ((rules
                                (append
                                 '((holiday-fixed
                                    2
                                    30
                                    "Impossible Fixed Date")
                                   (undefined-australia-holiday-rule
                                    "Broken"))
                                 australia-holidays))
                               warnings)
                           (cl-progv
                               '(calendar-holidays
                                 displayed-month
                                 displayed-year
                                 calendar-total-months)
                               (list
                                rules
                                1
                                2025
                                3)
                             (cl-letf
                                 (((symbol-function
                                    'display-warning)
                                   (lambda
                                       (type message &optional level buffer-name)
                                     (push
                                      (list
                                       type
                                       message
                                       level
                                       buffer-name)
                                      warnings)
                                     :warned)))
                               (list
                                (calendar-holiday-list)
                                (nreverse warnings)))))"##,
        expect![[
            r#"OK ((((12 25 2024) "Christmas Day") ((1 1 2025) "New Year") ((1 26 2025) "Australia Day") ((2 30 2025) "Impossible Fixed Date")) ((holidays "Bad holiday list item: (undefined-australia-holiday-rule Broken)\nError: (void-function undefined-australia-holiday-rule)\n" :error nil)))"#
        ]],
    )
}

pub(super) fn integration_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        australia_holidays_real_list_holidays_renders_complete_national_2025_buffer(),
        australia_holidays_real_list_holidays_renders_victorian_multi_year_schedule_and_label(),
        australia_holidays_custom_january_label_flows_into_real_rendered_holiday_buffer(),
        australia_holidays_can_append_to_existing_calendar_rules_without_losing_order_or_collisions(),
        australia_holidays_calendar_list_holidays_builds_real_three_month_boundary_buffer(),
        australia_holidays_holiday_in_range_is_inclusive_for_single_day_and_cross_month_queries(),
        australia_holidays_every_region_keeps_christmas_related_rules_on_literal_weekend_dates(),
        australia_holidays_bad_rule_isolated_by_calendar_engine_while_valid_package_rules_still_render(),
    ]
}
