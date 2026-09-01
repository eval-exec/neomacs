use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DATETIME_FORMAT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const DATETIME_FORMAT_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn datetime_format_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DATETIME_FORMAT_MELPA_PIN, "datetime-format.el")
        .expect("prepare pinned Datetime Format source below ./tmp")
        .with_timeout(DATETIME_FORMAT_TEST_TIMEOUT)
}

fn release_timestamp_renders_atom_http_mail_and_custom_protocol_dates() -> ParityBatchCase {
    ParityBatchCase::value(
        "release_timestamp_renders_atom_http_mail_and_custom_protocol_dates",
        r##"
(let ((released-at 1704067200))
  (list :atom-utc (datetime-format 'atom-utc released-at)
        :atom (datetime-format 'atom released-at :timezone "UTC")
        :cookie (datetime-format 'cookie released-at :timezone "UTC")
        :mail (datetime-format 'rfc-1123 released-at :timezone "UTC")
        :w3c (datetime-format 'w3c released-at :timezone "Asia/Tokyo")
        :custom (datetime-format "%Y/%m/%d %H:%M:%S %:z"
                                 released-at :timezone "Europe/Berlin")))
"##,
        expect![[
            r##"OK (:atom-utc "2024-01-01T00:00:00Z" :atom "2024-01-01T00:00:00+00:00" :cookie "Monday, 01-Jan-2024 00:00:00 UTC" :mail "Mon, 01 Jan 2024 00:00:00 +0000" :w3c "2024-01-01T09:00:00+09:00" :custom "2024/01/01 01:00:00 +01:00")"##
        ]],
    )
}

fn incident_timeline_crosses_new_yorks_spring_dst_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "incident_timeline_crosses_new_yorks_spring_dst_boundary",
        r##"
(let ((before 1710052200)
      (after 1710055800))
  (list :before-local
        (datetime-format 'atom before :timezone "America/New_York")
        :after-local
        (datetime-format 'atom after :timezone "America/New_York")
        :before-utc (datetime-format 'atom-utc before)
        :after-utc (datetime-format 'atom-utc after)
        :elapsed-seconds (- after before)))
"##,
        expect![[
            r##"OK (:before-local "2024-03-10T01:30:00-05:00" :after-local "2024-03-10T03:30:00-04:00" :before-utc "2024-03-10T06:30:00Z" :after-utc "2024-03-10T07:30:00Z" :elapsed-seconds 3600)"##
        ]],
    )
}

fn wall_clock_input_is_parsed_in_its_zone_and_tz_is_restored() -> ParityBatchCase {
    ParityBatchCase::value(
        "wall_clock_input_is_parsed_in_its_zone_and_tz_is_restored",
        r##"
(let ((saved (getenv "TZ")))
  (unwind-protect
      (progn
        (setenv "TZ" "Asia/Tokyo")
        (let ((berlin-wall
               (datetime-format 'atom "2024-07-15 09:30:00"
                                :timezone "Europe/Berlin"))
              (berlin-as-utc
               (datetime-format 'atom-utc "2024-07-15 09:30:00"
                                :timezone "Europe/Berlin"))
              (new-york-wall
               (datetime-format "%F %T %:z" "2024-07-15 09:30:00"
                                :timezone "America/New_York")))
          (list :berlin-wall berlin-wall
                :berlin-as-utc berlin-as-utc
                :new-york-wall new-york-wall
                :tz-after-calls (getenv "TZ"))))
    (setenv "TZ" saved)))
"##,
        expect![[
            r##"OK (:berlin-wall "2024-07-15T09:30:00+02:00" :berlin-as-utc "2024-07-15T07:30:00Z" :new-york-wall "2024-07-15 09:30:00 -04:00" :tz-after-calls "Asia/Tokyo")"##
        ]],
    )
}

fn scheduler_inputs_normalize_to_the_same_instant_without_leaking_tz() -> ParityBatchCase {
    ParityBatchCase::value(
        "scheduler_inputs_normalize_to_the_same_instant_without_leaking_tz",
        r##"
(let ((saved (getenv "TZ")))
  (unwind-protect
      (progn
        (setenv "TZ" "Europe/London")
        (let ((unix-time
               (datetime-format-convert-timestamp-dwim 1234567890))
              (new-york-wall
               (datetime-format-convert-timestamp-dwim
                "2009-02-13 18:31:30" "America/New_York"))
              (tokyo-wall
               (datetime-format-convert-timestamp-dwim
                "2009-02-14 08:31:30" "Asia/Tokyo")))
          (list :unix (datetime-format 'atom-utc unix-time)
                :new-york (datetime-format 'atom-utc new-york-wall)
                :tokyo (datetime-format 'atom-utc tokyo-wall)
                :same-instant
                (and (time-equal-p unix-time new-york-wall)
                     (time-equal-p new-york-wall tokyo-wall))
                :tz-after-conversion (getenv "TZ"))))
    (setenv "TZ" saved)))
"##,
        expect![[
            r##"OK (:unix "2009-02-13T23:31:30Z" :new-york "2009-02-13T23:31:30Z" :tokyo "2009-02-13T23:31:30Z" :same-instant t :tz-after-conversion "Europe/London")"##
        ]],
    )
}

fn unknown_named_format_is_rejected_at_the_public_boundary() -> ParityBatchCase {
    ParityBatchCase::signal(
        "unknown_named_format_is_rejected_at_the_public_boundary",
        r##"(datetime-format 'iso-8601-with-magic 1704067200 :timezone "UTC")"##,
        expect![[r##"ERR (error "‘iso-8601-with-magic’ is invalid time format name.")"##]],
    )
}

#[test]
fn datetime_format_package_batch() {
    let cases = vec![
        release_timestamp_renders_atom_http_mail_and_custom_protocol_dates(),
        incident_timeline_crosses_new_yorks_spring_dst_boundary(),
        wall_clock_input_is_parsed_in_its_zone_and_tz_is_restored(),
        scheduler_inputs_normalize_to_the_same_instant_without_leaking_tz(),
        unknown_named_format_is_rejected_at_the_public_boundary(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed Datetime Format parity test");
    assert_oracle_batch_cases(
        datetime_format_oracle(),
        test_name,
        "datetime_format_parity",
        &cases,
    );
}
