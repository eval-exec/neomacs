use std::time::Duration;

use expect_test::expect;

use crate::{CSV_MODE_GNU_ELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const CSV_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CSV_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
;; A global idle timer is unrelated to these deterministic editing workflows.
(setq csv-field-index-mode nil)
"##;

fn csv_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CSV_MODE_GNU_ELPA_PIN, "csv-mode.el")
        .expect("prepare pinned CSV Mode source below ./tmp")
        .with_prelude(CSV_MODE_TEST_PRELUDE)
        .with_timeout(CSV_MODE_TEST_TIMEOUT)
}

fn quoted_report_parsing_and_field_navigation_preserve_record_structure() -> ParityBatchCase {
    ParityBatchCase::value(
        "quoted_report_parsing_and_field_navigation_preserve_record_structure",
        r##"
(with-temp-buffer
  (insert "item,qty,note,owner\n"
          "\"Widget, Large\",12,\"He said \"\"go\"\"\",ops\n"
          "Cable,0,,\"Data, Inc.\"\n")
  (csv-mode)
  (let (rows navigation)
    (goto-char (point-min))
    (while (not (eobp))
      (push (csv-parse-current-row) rows)
      (forward-line 1))
    (goto-char (point-min))
    (forward-line 1)
    (let ((row-start (point)))
      (forward-sexp 1)
      (push (list :field 1
                  :index (csv--field-index)
                  :text (buffer-substring-no-properties row-start (point)))
            navigation)
      (forward-char 1)
      (let ((field-start (point)))
        (forward-sexp 2)
        (push (list :through-field 3
                    :index (csv--field-index)
                    :text (buffer-substring-no-properties field-start (point)))
              navigation)))
    (csv-header-line)
    (list :rows (nreverse rows)
          :navigation (nreverse navigation)
          :header (buffer-substring-no-properties
                   (overlay-start csv--header-line)
                   (overlay-end csv--header-line))
          :header-format-installed (local-variable-p 'header-line-format))))
"##,
        expect![[
            r##"OK (:rows (("item" "qty" "note" "owner") ("Widget, Large" "12" "He said \"go\"" "ops") ("Cable" "0" "" "Data, Inc.")) :navigation ((:field 1 :index 1 :text "\"Widget, Large\"") (:through-field 3 :index 3 :text "12,\"He said \"\"go\"\"\"")) :header "item,qty,note,owner" :header-format-installed t)"##
        ]],
    )
}

fn operators_move_name_and_status_before_team_while_preserving_comments() -> ParityBatchCase {
    ParityBatchCase::value(
        "operators_move_name_and_status_before_team_while_preserving_comments",
        r##"
(with-temp-buffer
  (insert "id,name,team,status\n"
          "1,Alice,platform,active\n"
          "# imported from HR\n"
          "2,\"Bob, Jr.\",data,on-leave\n"
          "3,Carla,finance,active\n")
  (csv-mode)
  (csv-kill-fields (list 2 4) (point-min) (point-max))
  (let ((after-cut (buffer-string))
        (clipboard (copy-sequence csv-killed-fields)))
    (csv-yank-fields 2 (point-min) (point-max))
    (list :clipboard clipboard
          :after-cut after-cut
          :restored (buffer-string)
          :restored-rows
          (save-excursion
            (goto-char (point-min))
            (let (rows)
              (while (not (eobp))
                (unless (csv-not-looking-at-record)
                  (push (csv-parse-current-row) rows))
                (forward-line 1))
              (nreverse rows))))))
"##,
        expect![[
            r##"OK (:clipboard ("name,status" "Alice,active" "\"Bob, Jr.\",on-leave" "Carla,active") :after-cut "id,team\n1,platform\n# imported from HR\n2,data\n3,finance\n" :restored "id,name,status,team\n1,Alice,active,platform\n# imported from HR\n2,\"Bob, Jr.\",on-leave,data\n3,Carla,active,finance\n" :restored-rows (("id" "name" "status" "team") ("1" "Alice" "active" "platform") ("2" "Bob, Jr." "on-leave" "data") ("3" "Carla" "active" "finance")))"##
        ]],
    )
}

fn inventory_sorting_handles_numeric_bases_comments_and_descending_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "inventory_sorting_handles_numeric_bases_comments_and_descending_names",
        r##"
(with-temp-buffer
  (insert "sku,name,quantity\n"
          "a1,Widget,010\n"
          "# quantities from warehouse\n"
          "b2,adapter,0x10\n"
          "c3,Cable,4\n"
          "d4,Battery,\n")
  (csv-mode)
  (let ((data-start (save-excursion
                      (goto-char (point-min))
                      (forward-line 1)
                      (point))))
    (csv-sort-numeric-fields 3 data-start (point-max))
    (let ((numeric (buffer-string))
          (sort-fold-case t)
          (csv-descending t))
      (csv-sort-fields 2 data-start (point-max))
      (list :numeric numeric
            :descending-name (buffer-string)))))
"##,
        expect![[
            r##"OK (:numeric "sku,name,quantity\nd4,Battery,\n# quantities from warehouse\nc3,Cable,4\na1,Widget,010\nb2,adapter,0x10\n" :descending-name "sku,name,quantity\na1,Widget,010\n# quantities from warehouse\nc3,Cable,4\nd4,Battery,\nb2,adapter,0x10\n")"##
        ]],
    )
}

fn hard_alignment_formats_quoted_rows_and_unalign_restores_canonical_csv() -> ParityBatchCase {
    ParityBatchCase::value(
        "hard_alignment_formats_quoted_rows_and_unalign_restores_canonical_csv",
        r##"
(with-temp-buffer
  (insert "sku,description,qty\n"
          "1,\"b\"\"c,\",3\n"
          "longer,plain,12\n"
          "# generated inventory\n")
  (csv-mode)
  (let ((csv-align-style 'auto)
        (csv-align-padding 2))
    (csv-align-fields t (point-min) (point-max))
    (let ((aligned (buffer-string))
          (separator-overlays
           (mapcar (lambda (overlay)
                     (list (buffer-substring-no-properties
                            (overlay-start overlay) (overlay-end overlay))
                           (overlay-get overlay 'invisible)))
                   (seq-filter (lambda (overlay) (overlay-get overlay 'csv))
                               (overlays-in (point-min) (point-max))))))
      (csv-unalign-fields t (point-min) (point-max))
      (list :aligned aligned
            :separator-overlays separator-overlays
            :unaligned (buffer-string)
            :remaining-csv-overlays
            (length (seq-filter
                     (lambda (overlay) (overlay-get overlay 'csv))
                     (overlays-in (point-min) (point-max))))))))
"##,
        expect![[
            r##"OK (:aligned "sku   ,  description,  qty\n     1,  \"b\"\"c,\"    ,    3\nlonger,  plain      ,   12\n# generated inventory\n" :separator-overlays (("," csv) ("," csv) ("," csv) ("," csv) ("," csv) ("," csv)) :unaligned "sku,description,qty\n1,\"b\"\"c,\",3\nlonger,plain,12\n# generated inventory\n" :remaining-csv-overlays 0)"##
        ]],
    )
}

fn semicolon_report_transposes_ragged_quoted_rows_and_round_trips() -> ParityBatchCase {
    ParityBatchCase::value(
        "semicolon_report_transposes_ragged_quoted_rows_and_round_trips",
        r##"
(with-temp-buffer
  (insert "name;Q1;Q2\n"
          "North;10;12\n"
          "\"West;Co\";8\n"
          "Total;18;12\n")
  (csv-mode)
  (csv-set-separator ?\;)
  (goto-char (point-min))
  (forward-line 2)
  (let ((quoted-row (csv-parse-current-row)))
    (csv-transpose (point-min) (point-max))
    (let ((transposed (buffer-string)))
      (csv-transpose (point-min) (point-max))
      (list :quoted-row quoted-row
            :transposed transposed
            :round-trip (buffer-string)
            :separator csv-separators))))
"##,
        expect![[
            r##"OK (:quoted-row ("West;Co" "8") :transposed "name;North;\"West;Co\";Total\nQ1;10;8;18\nQ2;12;;12\n" :round-trip "name;Q1;Q2\nNorth;10;12\n\"West;Co\";8\nTotal;18;12\n" :separator (";"))"##
        ]],
    )
}

#[test]
fn csv_mode_package_batch() {
    let cases = vec![
        quoted_report_parsing_and_field_navigation_preserve_record_structure(),
        operators_move_name_and_status_before_team_while_preserving_comments(),
        inventory_sorting_handles_numeric_bases_comments_and_descending_names(),
        hard_alignment_formats_quoted_rows_and_unalign_restores_canonical_csv(),
        semicolon_report_transposes_ragged_quoted_rows_and_round_trips(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed CSV Mode parity test");
    assert_oracle_batch_cases(csv_mode_oracle(), test_name, "csv_mode_parity", &cases);
}
