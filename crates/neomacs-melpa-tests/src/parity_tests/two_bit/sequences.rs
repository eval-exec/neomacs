use expect_test::expect;

use super::ParityBatchCase;

fn two_bit_sequence_loading_preserves_offsets_blocks_and_masking_policy() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_sequence_loading_preserves_offsets_blocks_and_masking_policy",
        r##"(let ((file
                    (expand-file-name
                     "sequences.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file)
                     (let* ((plain-data
                             (2bit-open file))
                            (plain
                             (2bit-sequence
                              plain-data
                              "alpha"))
                            (masked-data
                             (2bit-open file t))
                            (masked
                             (2bit-sequence
                              masked-data
                              "alpha")))
                       (list
                        (list
                         (2bit-sequence-name
                          plain)
                         (2bit-sequence-dna-size
                          plain)
                         (2bit-sequence-dna-offset
                          plain)
                         (2bit-block-collection-count
                          (2bit-sequence-n-blocks
                           plain))
                         (2bit-block-collection-starts
                          (2bit-sequence-n-blocks
                           plain))
                         (2bit-block-collection-sizes
                          (2bit-sequence-n-blocks
                           plain))
                         (2bit-sequence-mask-blocks
                          plain)
                         (eq
                          (2bit-sequence-source
                           plain)
                          plain-data))
                        (list
                         (2bit-sequence-name
                          masked)
                         (2bit-sequence-dna-size
                          masked)
                         (2bit-sequence-dna-offset
                          masked)
                         (2bit-block-collection-count
                          (2bit-sequence-mask-blocks
                           masked))
                         (2bit-block-collection-starts
                          (2bit-sequence-mask-blocks
                           masked))
                         (2bit-block-collection-sizes
                          (2bit-sequence-mask-blocks
                           masked))
                         (eq
                          (2bit-sequence-source
                           masked)
                          masked-data)))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect!["OK ((\"alpha\" 12 67 1 (2) (3) nil t) (\"alpha\" 12 67 1 (6) (4) t))"],
    )
}

fn two_bit_bases_decode_full_and_partial_ranges_across_byte_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_bases_decode_full_and_partial_ranges_across_byte_boundaries",
        r##"(let ((file
                    (expand-file-name
                     "bases.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file)
                     (let* ((data
                             (2bit-open file))
                            (alpha
                             (2bit-sequence
                              data "alpha"))
                            (beta
                             (2bit-sequence
                              data "beta")))
                       (list
                        (2bit-bases alpha 0 12)
                        (2bit-bases alpha 1 11)
                        (2bit-bases alpha 5 9)
                        (2bit-bases beta 0 8)
                        (2bit-bases beta 3 6)
                        (2bit-bases beta 7 8))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"OK ("TCNNNCAGTCAG" "CNNNCAGTCA" "CAGT" "GGGGAAAA" "GAA" "A")"#]],
    )
}

fn two_bit_bases_apply_mask_blocks_only_when_requested_at_open_time() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_bases_apply_mask_blocks_only_when_requested_at_open_time",
        r##"(let ((file
                    (expand-file-name
                     "masking.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file 'big)
                     (let ((plain
                            (2bit-sequence
                             (2bit-open file)
                             "alpha"))
                           (masked
                            (2bit-sequence
                             (2bit-open file t)
                             "alpha")))
                       (list
                        (2bit-bases plain 0 12)
                        (2bit-bases masked 0 12)
                        (2bit-bases masked 5 11))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"OK ("TCNNNCAGTCAG" "TCNNNCagtcAG" "CagtcA")"#]],
    )
}

fn two_bit_sequence_rejects_an_unknown_name_with_exact_signal_data() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_sequence_rejects_an_unknown_name_with_exact_signal_data",
        r##"(let ((file
                    (expand-file-name
                     "unknown.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file)
                     (2bit-sequence
                      file "gamma"))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"ERR (error "Unknown sequence \"gamma\"")"#]],
    )
}

fn two_bit_bases_reject_equal_or_reversed_bounds() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_bases_reject_equal_or_reversed_bounds",
        r##"(let ((sequence
                    (make-2bit-sequence
                     :dna-size 12)))
               (2bit-bases sequence 5 5))"##,
        expect![[r#"ERR (error "Start location is greater or equal to the end location")"#]],
    )
}

fn two_bit_bases_reject_a_negative_start() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_bases_reject_a_negative_start",
        r##"(let ((sequence
                    (make-2bit-sequence
                     :dna-size 12)))
               (2bit-bases sequence -1 2))"##,
        expect![[r#"ERR (error "Start location is less than 0")"#]],
    )
}

fn two_bit_bases_reject_a_start_at_the_sequence_end() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_bases_reject_a_start_at_the_sequence_end",
        r##"(let ((sequence
                    (make-2bit-sequence
                     :dna-size 12)))
               (2bit-bases sequence 12 13))"##,
        expect![[r#"ERR (error "Start location is beyond the end of the sequence")"#]],
    )
}

fn two_bit_bases_reject_an_end_beyond_the_sequence() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_bases_reject_an_end_beyond_the_sequence",
        r##"(let ((sequence
                    (make-2bit-sequence
                     :dna-size 12)))
               (2bit-bases sequence 11 13))"##,
        expect![[r#"ERR (error "End location is beyond the end of the sequence")"#]],
    )
}

fn two_bit_with_file_and_sequence_macros_bind_fresh_readers_for_the_body() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_with_file_and_sequence_macros_bind_fresh_readers_for_the_body",
        r##"(let ((file
                    (expand-file-name
                     "macros.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file)
                     (list
                      (2bit-with-file
                          (data file t)
                        (list
                         (2bit-data-p data)
                         (2bit-data-masking data)
                         (2bit-sequence-count
                          data)))
                      (2bit-with-sequence
                          (sequence "beta" file)
                        (list
                         (2bit-sequence-p
                          sequence)
                         (2bit-sequence-name
                          sequence)
                         (2bit-sequence-dna-size
                          sequence)
                         (2bit-bases
                          sequence 0 8)))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"OK ((t t 2) (t "beta" 8 "GGGGAAAA"))"#]],
    )
}

fn two_bit_macroexpansions_evaluate_file_and_sequence_forms_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_macroexpansions_evaluate_file_and_sequence_forms_once",
        r##"(list
              (macroexpand-1
               '(2bit-with-file
                    (data (progn
                            (push 'file events)
                            path)
                          t)
                  (2bit-sequence-count data)))
              (macroexpand-1
               '(2bit-with-sequence
                    (sequence
                     (progn
                       (push 'name events)
                       "alpha")
                     (progn
                       (push 'file events)
                       path))
                  (2bit-sequence-dna-size
                   sequence))))"##,
        expect![[
            r#"OK ((let ((data (2bit-open (progn (push 'file events) path) t))) (2bit-sequence-count data)) (let ((sequence (2bit-sequence (2bit-open (progn (push 'file events) path)) (progn (push 'name events) "alpha")))) (2bit-sequence-dna-size sequence)))"#
        ]],
    )
}

pub(super) fn sequences_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        two_bit_sequence_loading_preserves_offsets_blocks_and_masking_policy(),
        two_bit_bases_decode_full_and_partial_ranges_across_byte_boundaries(),
        two_bit_bases_apply_mask_blocks_only_when_requested_at_open_time(),
        two_bit_sequence_rejects_an_unknown_name_with_exact_signal_data(),
        two_bit_bases_reject_equal_or_reversed_bounds(),
        two_bit_bases_reject_a_negative_start(),
        two_bit_bases_reject_a_start_at_the_sequence_end(),
        two_bit_bases_reject_an_end_beyond_the_sequence(),
        two_bit_with_file_and_sequence_macros_bind_fresh_readers_for_the_body(),
        two_bit_macroexpansions_evaluate_file_and_sequence_forms_once(),
    ]
}
