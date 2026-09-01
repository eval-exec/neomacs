use expect_test::expect;

use super::ParityBatchCase;

fn parsing_a_pasted_spreadsheet_export_into_records() -> ParityBatchCase {
    ParityBatchCase::value(
        "parsing_a_pasted_spreadsheet_export_into_records",
        r##"
        ;; The commonest thing anyone uses `s' for: text pasted out of a
        ;; spreadsheet, split into records and cleaned up.  The export carries
        ;; everything that makes this awkward - CRLF from the download with one
        ;; hand-edited LF line, a trailing blank line, cells padded with spaces
        ;; and tabs, an empty field, runs of internal whitespace that must be
        ;; collapsed but not stripped, and names outside ASCII.  The workflow
        ;; runs the pipeline a program would and asserts the records whole.
        (let* ((lines (s-lines (s-trim s-test-export)))
               (rows (mapcar (lambda (line) (s-split "," line)) lines))
               (header (mapcar #'s-trim (car rows)))
               (records (mapcar (lambda (row)
                                  (cl-mapcar (lambda (key cell)
                                               (cons (s-test-copy key)
                                                     (s-collapse-whitespace (s-trim cell))))
                                             header row))
                                (cdr rows))))
          (list
           :line-count (length lines)
           :blank-line-survived-the-trim (s-blank-p (car (last lines)))
           :header header
           :records records
           :prices (mapcar (lambda (record) (cdr (assoc "price" record))) records)
           :empty-name-kept-as-a-field
           (let ((record (nth 3 records)))
             (list (cdr (assoc "name" record)) (s-blank-p (cdr (assoc "name" record)))))
           :internal-spacing-collapsed (cdr (assoc "note" (nth 1 records)))
           :splitting-without-omitting-nulls
           (s-split "," "WH-004,,0.00,missing name")
           :splitting-while-omitting-nulls
           (s-split "," "WH-004,,0.00,missing name" t)))
    "##,
        expect![[
            r#"OK (:line-count 5 :blank-line-survived-the-trim nil :header ("sku" "name" "price" "note") :records ((("sku" . "WH-001") ("name" . "Gruesse Widget") ("price" . "12.50") ("note" . "")) (("sku" . "WH-002") ("name" . "ねじ回し") ("price" . "3.00") ("note" . "keeps its spacing")) (("sku" . "WH-003") ("name" . "Café Cup") ("price" . "7.25") ("note" . "naive")) (("sku" . "WH-004") ("name" . "") ("price" . "0.00") ("note" . "missing name"))) :prices ("12.50" "3.00" "7.25" "0.00") :empty-name-kept-as-a-field ("" t) :internal-spacing-collapsed "keeps its spacing" :splitting-without-omitting-nulls ("WH-004" "" "0.00" "missing name") :splitting-while-omitting-nulls ("WH-004" "0.00" "missing name"))"#
        ]],
    )
}

fn laying_out_a_report_column_counts_characters_not_display_width() -> ParityBatchCase {
    ParityBatchCase::value(
        "laying_out_a_report_column_counts_characters_not_display_width",
        r##"
        ;; Rendering the parsed rows as a fixed-width column is where a string
        ;; library's idea of "length" becomes visible.  `s' pads and truncates
        ;; by *character count*, so a name of full-width CJK characters occupies
        ;; twice the columns of an ASCII name padded to the same number - which
        ;; is the behaviour, not a bug, and is exactly the kind of thing two
        ;; implementations could disagree about.  The fixture puts an ASCII
        ;; name, an accented name and a CJK name side by side so the difference
        ;; between `length' and `string-width' shows in the output.
        (let ((names '("Cup" "Café Cup" "ねじ回し" "already exactly twelve")))
          (list
           :character-lengths (mapcar #'length names)
           :display-widths (mapcar #'string-width names)
           :padded-right (mapcar (lambda (name) (s-pad-right 12 "." name)) names)
           :padded-left (mapcar (lambda (name) (s-pad-left 12 "." name)) names)
           :centered (mapcar (lambda (name) (s-center 12 name)) names)
           :truncated (mapcar (lambda (name) (s-truncate 8 name)) names)
           :truncated-with-custom-ellipsis
           (mapcar (lambda (name) (s-truncate 8 name "…")) names)
           :left-and-right (mapcar (lambda (name) (list (s-left 4 name) (s-right 4 name)))
                                   names)
           :wrapped (s-word-wrap 20 "keeps its spacing across a wrapped note field")))
    "##,
        expect![[
            r#"OK (:character-lengths (3 8 4 22) :display-widths (3 8 8 22) :padded-right ("Cup........." "Café Cup...." "ねじ回し........" "already exactly twelve") :padded-left (".........Cup" "....Café Cup" "........ねじ回し" "already exactly twelve") :centered ("     Cup    " "  Café Cup  " "    ねじ回し    " "already exactly twelve") :truncated ("Cup" "Café Cup" "ねじ回し" "alrea...") :truncated-with-custom-ellipsis ("Cup" "Café Cup" "ねじ回し" "already…") :left-and-right (("Cup" "Cup") ("Café" " Cup") ("ねじ回し" "ねじ回し") ("alre" "elve")) :wrapped "keeps its spacing\nacross a wrapped\nnote field")"#
        ]],
    )
}

fn filling_a_message_template_from_the_parsed_record() -> ParityBatchCase {
    ParityBatchCase::value(
        "filling_a_message_template_from_the_parsed_record",
        r##"
        ;; `s-format' is the function most other packages in this tree reach
        ;; for, and it has three different replacer protocols plus its own
        ;; error.  The workflow fills one realistic message through all of
        ;; them - an alist lookup, a hash table, a function - checks that a
        ;; literal `$' is left alone, and pins the signal raised when a
        ;; placeholder cannot be resolved, since that is what a caller sees
        ;; when its data is missing a key.
        (let* ((record '(("sku" . "WH-002") ("name" . "ねじ回し") ("price" . "3.00")))
               (table (let ((hash (make-hash-table :test 'equal)))
                        (dolist (pair record) (puthash (car pair) (cdr pair) hash))
                        hash))
               (template "${name} (${sku}) costs $${price}"))
          (list
           :from-an-alist (s-format template 'aget record)
           :from-a-hash-table (s-format template 'gethash table)
           :from-a-function (s-format template
                                      (lambda (key) (upcase (or (cdr (assoc key record)) ""))))
           :by-index (s-format "$0 then $1" 'elt '("first" "second"))
           ;; `$$' is s-format's escape for a literal dollar, but it is not
           ;; collapsed on output, and a digit after it is read as an index
           ;; form rather than as text - so the documented escape does not
           ;; survive being followed by a number.
           :escaped-dollar-is-left-doubled (s-format "costs $$ each" 'aget record)
           :escaped-dollar-before-a-digit
           (condition-case error (s-format "costs $$5" 'aget record)
             (error (list :signal (car error) :data (cdr error))))
           :missing-key
           (condition-case error (s-format "${absent}" 'aget record)
             (error (list :signal (car error) :data (cdr error))))
           :lex-format
           (let ((name "Café Cup") (price "7.25"))
             (s-lex-format "${name} at ${price}"))))
    "##,
        expect![[
            r#"OK (:from-an-alist "ねじ回し (WH-002) costs $3.00" :from-a-hash-table "ねじ回し (WH-002) costs $3.00" :from-a-function "ねじ回し (WH-002) costs $3.00" :by-index "first then second" :escaped-dollar-is-left-doubled "costs $$ each" :escaped-dollar-before-a-digit (:signal wrong-type-argument :data (stringp 5)) :missing-key (:signal s-format-resolve :data "${absent}") :lex-format "Café Cup at 7.25")"#
        ]],
    )
}

fn turning_typed_headings_into_slugs_and_titles() -> ParityBatchCase {
    ParityBatchCase::value(
        "turning_typed_headings_into_slugs_and_titles",
        r##"
        ;; Headings typed by a person become anchors, file names and titles.
        ;; The interesting cases are all in the fixture: leading and trailing
        ;; space, internal punctuation, an acronym that case conversion must
        ;; not mangle, a heading that is already dashed, one with no spaces at
        ;; all, and one that is only punctuation.  Each heading goes through
        ;; every conversion so the whole family can be compared row by row.
        (s-test-report
         (mapcar (lambda (heading)
                   (cons heading
                         (list :dashed (s-dashed-words heading)
                               :snake (s-snake-case heading)
                               :lower-camel (s-lower-camel-case heading)
                               :upper-camel (s-upper-camel-case heading)
                               :capitalized (s-capitalize heading)
                               :titleized (s-titleize heading)
                               :words (s-split-words heading))))
                 s-test-headings))
    "##,
        expect![[
            r#"OK (("  Getting Started with Widgets  " (:dashed "getting-started-with-widgets" :snake "getting_started_with_widgets" :lower-camel "gettingStartedWithWidgets" :upper-camel "GettingStartedWithWidgets" :capitalized "  getting started with widgets  " :titleized "  Getting Started With Widgets  " :words ("Getting" "Started" "with" "Widgets"))) ("API reference: the HTTP endpoints" (:dashed "api-reference-the-http-endpoints" :snake "api_reference_the_http_endpoints" :lower-camel "apiReferenceTheHttpEndpoints" :upper-camel "ApiReferenceTheHttpEndpoints" :capitalized "Api reference: the http endpoints" :titleized "Api Reference: The Http Endpoints" :words ("API" "reference" "the" "HTTP" "endpoints"))) ("Gruesse & Groessen" (:dashed "gruesse-groessen" :snake "gruesse_groessen" :lower-camel "gruesseGroessen" :upper-camel "GruesseGroessen" :capitalized "Gruesse & groessen" :titleized "Gruesse & Groessen" :words ("Gruesse" "Groessen"))) ("already-dashed-heading" (:dashed "already-dashed-heading" :snake "already_dashed_heading" :lower-camel "alreadyDashedHeading" :upper-camel "AlreadyDashedHeading" :capitalized "Already-dashed-heading" :titleized "Already-Dashed-Heading" :words ("already" "dashed" "heading"))) ("MixedCASE wordsHere" (:dashed "mixed-case-words-here" :snake "mixed_case_words_here" :lower-camel "mixedCaseWordsHere" :upper-camel "MixedCaseWordsHere" :capitalized "Mixedcase wordshere" :titleized "Mixedcase Wordshere" :words ("Mixed" "CASE" "words" "Here"))) ("---" (:dashed "" :snake "" :lower-camel "" :upper-camel "" :capitalized "---" :titleized "---" :words nil)))"#
        ]],
    )
}

fn extracting_fields_from_log_lines_with_capture_groups() -> ParityBatchCase {
    ParityBatchCase::value(
        "extracting_fields_from_log_lines_with_capture_groups",
        r##"
        ;; Reading structured lines out of log output is the other half of what
        ;; `s' is used for, and it is the half that touches match data.  The
        ;; fixture has four lines that fit the format and one continuation line
        ;; that does not, so the workflow shows both what is extracted and what
        ;; is correctly skipped.  `s-match' is asserted alongside
        ;; `s-match-strings-all' and `s-matched-positions-all' because the
        ;; three report the same matches in three different shapes.
        (let* ((pattern "^\\([0-9-]+\\) \\([0-9:]+\\) \\([A-Z]+\\) +\\[\\([a-z]+\\)\\] \\(.*\\)$")
               (lines (s-lines (s-trim s-test-log)))
               (parsed (mapcar (lambda (line)
                                 (let ((fields (s-match pattern line)))
                                   (if fields
                                       (list :level (nth 3 fields)
                                             :subsystem (nth 4 fields)
                                             :message (nth 5 fields))
                                     (list :unparsed (s-trim line)))))
                               lines)))
          (list
           :parsed (s-test-copy-tree parsed)
           :levels (s-test-copy-tree
                    (mapcar #'cadr (s-match-strings-all "\\b\\(INFO\\|WARN\\|ERROR\\)\\b"
                                                        s-test-log)))
           :subsystem-positions (s-matched-positions-all "\\[\\([a-z]+\\)\\]" s-test-log 1)
           :durations (s-test-copy-tree
                       (s-match-strings-all "in \\([0-9]+\\)ms" s-test-log))
           :match-with-a-start-offset
           (s-test-copy-tree (s-match "\\[\\([a-z]+\\)\\]" s-test-log 100))
           :errors-only (s-test-copy-tree
                         (seq-filter (lambda (line) (s-contains-p "ERROR" line)) lines))
           :counting (list (s-count-matches "INFO" s-test-log)
                           (s-count-matches "widgets" s-test-log))))
    "##,
        expect![[
            r#"OK (:parsed ((:level "INFO" :subsystem "inventory" :message "loaded 3 widgets in 12ms") (:level "WARN" :subsystem "sync" :message "bucket cache is stale (age=91s)") (:level "ERROR" :subsystem "sync" :message "upload failed: connection refused") (:unparsed "at com.warehouse.Sync.upload(Sync.java:42)") (:level "INFO" :subsystem "inventory" :message "reloaded 3 widgets in 9ms")) :levels ("INFO" "WARN" "ERROR" "INFO") :subsystem-positions ((27 . 36) (90 . 94) (155 . 159) (267 . 276)) :durations (("in 12ms" "12") ("in 9ms" "9")) :match-with-a-start-offset ("[sync]" "sync") :errors-only ("2026-07-28 09:14:04 ERROR [sync] upload failed: connection refused") :counting (2 2))"#
        ]],
    )
}

fn deciding_whether_a_submitted_field_counts_as_filled_in() -> ParityBatchCase {
    ParityBatchCase::value(
        "deciding_whether_a_submitted_field_counts_as_filled_in",
        r##"
        ;; An empty string, a whitespace-only string and nil are three
        ;; different things to `s-blank-p' and `s-present-p', and `s-numeric-p'
        ;; rejects a non-ASCII digit that `string-to-number' would also refuse.
        ;; Running the whole submitted form through every predicate at once
        ;; makes those boundaries visible as a table rather than as separate
        ;; claims.
        (list
         :fields (s-test-report
                  (mapcar (lambda (field)
                            (cons (car field)
                                  ;; `s-presence' hands back the argument
                                  ;; itself, so it and `:value' would be one
                                  ;; object and print as a back reference.
                                  (list :value (s-test-copy (cdr field))
                                        :blank (s-blank-p (cdr field))
                                        :present (s-present-p (cdr field))
                                        :numeric (s-numeric-p (cdr field))
                                        :presence (s-test-copy (s-presence (cdr field))))))
                          s-test-form))
         :nil-is-blank-and-absent (list (s-blank-p nil) (s-present-p nil) (s-presence nil)))
    "##,
        expect![[
            r#"OK (:fields (("sku" (:value "WH-001" :blank nil :present t :numeric nil :presence "WH-001")) ("quantity" (:value "12" :blank nil :present t :numeric t :presence "12")) ("discount" (:value "" :blank t :present nil :numeric nil :presence nil)) ("note" (:value "   " :blank nil :present t :numeric nil :presence "   ")) ("price" (:value "12.50" :blank nil :present t :numeric nil :presence "12.50")) ("owner" (:value "Zoë" :blank nil :present t :numeric nil :presence "Zoë")) ("count" (:value "٣" :blank nil :present t :numeric nil :presence "٣"))) :nil-is-blank-and-absent (t nil nil))"#
        ]],
    )
}

fn matching_prefixes_suffixes_and_shared_edges_of_a_sku() -> ParityBatchCase {
    ParityBatchCase::value(
        "matching_prefixes_suffixes_and_shared_edges_of_a_sku",
        r##"
        ;; The other half of validating input: recognising and stripping the
        ;; fixed parts of an identifier, with case sensitivity pinned both ways
        ;; because that argument is easy to get backwards, and a chop that does
        ;; not apply asserted alongside one that does.
        (let ((sku (cdr (assoc "sku" s-test-form))))
          (list
           :starts (s-starts-with-p "WH-" sku)
           :starts-ignoring-case (s-starts-with-p "wh-" sku t)
           :starts-case-sensitive (s-starts-with-p "wh-" sku)
           :ends (s-ends-with-p "001" sku)
           :contains (s-contains-p "H-0" sku)
           :chopped (s-test-copy (s-chop-prefix "WH-" sku))
           :chop-that-does-not-apply (s-test-copy (s-chop-prefix "XX-" sku))
           :shared-start (s-test-copy (s-shared-start "WH-001" "WH-004"))
           :shared-end (s-test-copy (s-shared-end "12.50" "3.50"))
           :nothing-shared (s-test-copy (s-shared-start "WH-001" "nothing"))))
    "##,
        expect![[
            r#"OK (:starts t :starts-ignoring-case t :starts-case-sensitive nil :ends t :contains t :chopped "001" :chop-that-does-not-apply "WH-001" :shared-start "WH-00" :shared-end ".50" :nothing-shared "")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        parsing_a_pasted_spreadsheet_export_into_records(),
        laying_out_a_report_column_counts_characters_not_display_width(),
        filling_a_message_template_from_the_parsed_record(),
        turning_typed_headings_into_slugs_and_titles(),
        extracting_fields_from_log_lines_with_capture_groups(),
        deciding_whether_a_submitted_field_counts_as_filled_in(),
        matching_prefixes_suffixes_and_shared_edges_of_a_sku(),
    ]
}
