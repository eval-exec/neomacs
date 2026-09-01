use expect_test::expect;

use super::ParityBatchCase;

fn two_bit_location_prompt_parses_dash_and_dot_ranges_without_numeric_prompts() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_location_prompt_parses_dash_and_dot_ranges_without_numeric_prompts",
        r##"(let ((selections
                    '("alpha:2-9"
                      "beta:1..7"))
                   events)
               (cl-letf
                   (((symbol-function
                      'read-file-name)
                     (lambda (prompt)
                       (push
                        (list 'file prompt)
                        events)
                       "genome.2bit"))
                    ((symbol-function
                      '2bit-sequence-names)
                     (lambda (file)
                       (push
                        (list 'names file)
                        events)
                       '("alpha" "beta")))
                    ((symbol-function
                      'completing-read)
                     (lambda (prompt collection
                              &rest _)
                       (push
                        (list
                         'complete prompt
                         (copy-sequence
                          collection))
                        events)
                       (prog1
                           (car selections)
                         (setq selections
                               (cdr selections)))))
                    ((symbol-function
                      'read-number)
                     (lambda (&rest _)
                       (error
                        "numeric prompt should not run"))))
                 (list
                  (2bit--location-prompt)
                  (2bit--location-prompt)
                  (nreverse events))))"##,
        expect![[
            r#"OK (("genome.2bit" "alpha" 2 9) ("genome.2bit" "beta" 1 7) ((file "2bit file: ") (names "genome.2bit") (complete "Sequence: " ("alpha" "beta")) (file "2bit file: ") (names "genome.2bit") (complete "Sequence: " ("alpha" "beta"))))"#
        ]],
    )
}

fn two_bit_location_prompt_reads_start_and_defaults_end_to_sequence_size() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_location_prompt_reads_start_and_defaults_end_to_sequence_size",
        r##"(let ((numbers '(3 8))
                   events)
               (cl-letf
                   (((symbol-function
                      'read-file-name)
                     (lambda (_) "genome.2bit"))
                    ((symbol-function
                      '2bit-sequence-names)
                     (lambda (_) '("alpha")))
                    ((symbol-function
                      'completing-read)
                     (lambda (&rest _) "alpha"))
                    ((symbol-function
                      '2bit-sequence)
                     (lambda (file name)
                       (push
                        (list 'sequence file name)
                        events)
                       (make-2bit-sequence
                        :dna-size 12)))
                    ((symbol-function
                      'read-number)
                     (lambda (prompt default)
                       (push
                        (list
                         'number prompt default)
                        events)
                       (prog1
                           (car numbers)
                         (setq numbers
                               (cdr numbers))))))
                 (list
                  (2bit--location-prompt)
                  (nreverse events))))"##,
        expect![[
            r#"OK (("genome.2bit" "alpha" 3 8) ((number "alpha; Start: " 0) (sequence "genome.2bit" "alpha") (number "alpha; Start: 3; End: " 12)))"#
        ]],
    )
}

fn two_bit_location_prompt_restores_the_callers_match_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_location_prompt_restores_the_callers_match_data",
        r##"(progn
               (string-match
                "\\(outer\\)" "outer")
               (let ((before
                      (match-data t)))
                 (cl-letf
                     (((symbol-function
                        'read-file-name)
                       (lambda (_) "genome.2bit"))
                      ((symbol-function
                        '2bit-sequence-names)
                       (lambda (_) '("alpha")))
                      ((symbol-function
                        'completing-read)
                       (lambda (&rest _)
                         "alpha:2..9")))
                   (list
                    (2bit--location-prompt)
                    before
                    (match-data t)))))"##,
        expect![[r#"OK (("genome.2bit" "alpha" 2 9) (0 5 0 5) (0 5 0 5))"#]],
    )
}

fn two_bit_insert_bases_inserts_at_point_and_forwards_prefix_masking() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_insert_bases_inserts_at_point_and_forwards_prefix_masking",
        r##"(let ((file
                    (expand-file-name
                     "insert.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file)
                     (with-temp-buffer
                       (insert "before|after")
                       (goto-char 8)
                       (let
                           ((current-prefix-arg
                             '(4)))
                         (list
                          (2bit-insert-bases
                           file "alpha" 0 12)
                          (buffer-string)
                          (point)))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"OK (nil "before|TCNNNCagtcAGafter" 20)"#]],
    )
}

fn two_bit_insert_fasta_formats_header_and_short_sequence() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_insert_fasta_formats_header_and_short_sequence",
        r##"(let ((file
                    (expand-file-name
                     "sample.genome.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file)
                     (with-temp-buffer
                       (list
                        (2bit-insert-fasta
                         file "beta" 0 8)
                        (buffer-string)
                        (point))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"OK (nil "> sample.genome; beta:0-8\nGGGGAAAA\n" 36)"#]],
    )
}

fn two_bit_insert_fasta_wraps_every_complete_eighty_character_chunk() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_insert_fasta_wraps_every_complete_eighty_character_chunk",
        r##"(cl-letf
              (((symbol-function '2bit-open)
                (lambda (&rest _)
                  'data))
               ((symbol-function
                 '2bit-sequence)
                (lambda (&rest _)
                  'sequence))
               ((symbol-function '2bit-bases)
                (lambda (&rest _)
                  (concat
                   (make-string 80 ?A)
                   (make-string 80 ?C)
                   "GG")))
               ((symbol-function
                 'file-name-base)
                (lambda (_) "fixture")))
              (with-temp-buffer
                (list
                 (2bit-insert-fasta
                  "ignored.2bit"
                  "chr1" 10 172)
                 (buffer-string))))"##,
        expect![[
            r#"OK (nil "> fixture; chr1:10-172\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC\nGG\n")"#
        ]],
    )
}

fn two_bit_insert_commands_publish_the_exact_interactive_argument_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_insert_commands_publish_the_exact_interactive_argument_contract",
        r##"(list
              (interactive-form
               '2bit-insert-bases)
              (interactive-form
               '2bit-insert-fasta)
              (get
               '2bit-open
               'function-documentation)
              (commandp
               '2bit-insert-bases)
              (commandp
               '2bit-insert-fasta))"##,
        expect![[
            r#"OK ((interactive (2bit--location-prompt)) (interactive (2bit--location-prompt)) nil t t)"#
        ]],
    )
}

pub(super) fn commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        two_bit_location_prompt_parses_dash_and_dot_ranges_without_numeric_prompts(),
        two_bit_location_prompt_reads_start_and_defaults_end_to_sequence_size(),
        two_bit_location_prompt_restores_the_callers_match_data(),
        two_bit_insert_bases_inserts_at_point_and_forwards_prefix_masking(),
        two_bit_insert_fasta_formats_header_and_short_sequence(),
        two_bit_insert_fasta_wraps_every_complete_eighty_character_chunk(),
        two_bit_insert_commands_publish_the_exact_interactive_argument_contract(),
    ]
}
