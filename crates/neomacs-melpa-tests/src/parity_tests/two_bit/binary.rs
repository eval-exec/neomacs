use expect_test::expect;

use super::ParityBatchCase;

fn two_bit_public_constants_and_struct_defaults_match_the_pinned_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_public_constants_and_struct_defaults_match_the_pinned_release",
        r##"(let ((data (make-2bit-data))
                     (blocks
                      (make-2bit-block-collection))
                     (sequence
                      (make-2bit-sequence)))
               (list
                2bit-signature
                2bit-bases
                (2bit-data-p data)
                (mapcar
                 (lambda (accessor)
                   (funcall accessor data))
                 '(2bit-data-source
                   2bit-data-signature
                   2bit-data-other-endian-p
                   2bit-data-version
                   2bit-data-sequence-count
                   2bit-data-masking
                   2bit-data-index
                   2bit-data-pos))
                (2bit-block-collection-p
                 blocks)
                (list
                 (2bit-block-collection-count
                  blocks)
                 (2bit-block-collection-starts
                  blocks)
                 (2bit-block-collection-sizes
                  blocks))
                (2bit-sequence-p sequence)
                (list
                 (2bit-sequence-source sequence)
                 (2bit-sequence-name sequence)
                 (2bit-sequence-dna-size
                  sequence)
                 (2bit-sequence-n-blocks
                  sequence)
                 (2bit-sequence-mask-blocks
                  sequence)
                 (2bit-sequence-dna-offset
                  sequence))))"##,
        expect![[
            r#"OK (440477507 ["T" "C" "A" "G"] t (nil nil nil nil nil nil nil nil) t (nil nil nil) t (nil nil nil nil nil nil))"#
        ]],
    )
}

fn two_bit_relevant_blocks_observe_intersections_and_boundary_quirks() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_relevant_blocks_observe_intersections_and_boundary_quirks",
        r##"(let ((blocks
                    (make-2bit-block-collection
                     :count 5
                     :starts '(0 4 8 10 20)
                     :sizes '(2 4 2 5 0))))
               (list
                (2bit--relevant-blocks
                 5 10 blocks)
                (2bit--relevant-blocks
                 2 4 blocks)
                (2bit--relevant-blocks
                 10 10 blocks)
                (2bit--relevant-blocks
                 0 100 nil)))"##,
        expect!["OK (((4 . 8) (8 . 10) (10 . 15)) ((4 . 8)) ((10 . 15)) nil)"],
    )
}

fn two_bit_cursor_read_and_skip_preserve_binary_bytes_and_exact_position() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_cursor_read_and_skip_preserve_binary_bytes_and_exact_position",
        r##"(let* ((file
                      (expand-file-name
                       "cursor.bin"
                       (getenv "TMPDIR")))
                     (source
                      (make-2bit-data
                       :source file
                       :pos 0)))
               (unwind-protect
                   (progn
                     (with-temp-buffer
                       (set-buffer-multibyte nil)
                       (insert
                        (unibyte-string
                         0 127 128 255 65 66))
                       (let
                           ((coding-system-for-write
                             'binary))
                         (write-region
                          (point-min)
                          (point-max)
                          file nil 'silent)))
                     (list
                      (string-to-list
                       (2bit--read source 4))
                      (2bit-data-pos source)
                      (2bit--goto source 1)
                      (2bit--skip source 2)
                      (2bit-data-pos source)
                      (string-to-list
                       (2bit--read source 3))
                      (2bit-data-pos source)))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect!["OK ((0 127 128 255) 4 1 3 3 (255 65 66) 6)"],
    )
}

fn two_bit_word_helpers_decode_little_and_big_endian_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_word_helpers_decode_little_and_big_endian_values",
        r##"(let ((little
                    (make-2bit-data
                     :other-endian-p nil))
                   (big
                    (make-2bit-data
                     :other-endian-p t))
                   (bytes
                    (unibyte-string
                     120 86 52 18)))
               (list
                (2bit--word-swap
                 305419896)
                (2bit--word-swap
                 (2bit--word-swap
                  305419896))
                (2bit--word-from-bytes
                 little bytes)
                (2bit--word-from-bytes
                 big bytes)
                (2bit--word-from-bytes
                 big
                 (unibyte-string
                  18 52 86 120))))"##,
        expect!["OK (2018915346 305419896 305419896 2018915346 305419896)"],
    )
}

fn two_bit_read_word_and_words_advance_once_over_contiguous_data() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_read_word_and_words_advance_once_over_contiguous_data",
        r##"(let* ((file
                      (expand-file-name
                       "words.bin"
                       (getenv "TMPDIR")))
                     (source
                      (make-2bit-data
                       :source file
                       :pos 0)))
               (unwind-protect
                   (progn
                     (with-temp-buffer
                       (set-buffer-multibyte nil)
                       (insert
                        (neomacs-2bit--word
                         1 nil)
                        (neomacs-2bit--word
                         305419896 nil)
                        (neomacs-2bit--word
                         4294967295 nil))
                       (let
                           ((coding-system-for-write
                             'binary))
                         (write-region
                          (point-min)
                          (point-max)
                          file nil 'silent)))
                     (list
                      (2bit--read-word source)
                      (2bit-data-pos source)
                      (2bit--read-words
                       source 2)
                      (2bit-data-pos source)
                      (2bit--read-words
                       source 0)
                      (2bit-data-pos source)))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect!["OK (1 4 (305419896 4294967295) 12 nil 12)"],
    )
}

fn two_bit_index_reader_decodes_names_offsets_and_uses_worst_case_read_size() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_index_reader_decodes_names_offsets_and_uses_worst_case_read_size",
        r##"(let* ((file
                      (expand-file-name
                       "index.bin"
                       (getenv "TMPDIR")))
                     (source
                      (make-2bit-data
                       :source file
                       :sequence-count 2
                       :pos 0)))
               (unwind-protect
                   (progn
                     (with-temp-buffer
                       (set-buffer-multibyte nil)
                       (insert
                        (unibyte-string 1)
                        "a"
                        (neomacs-2bit--word
                         17 nil)
                        (unibyte-string 3)
                        "xyz"
                        (neomacs-2bit--word
                         4096 nil))
                       (let
                           ((coding-system-for-write
                             'binary))
                         (write-region
                          (point-min)
                          (point-max)
                          file nil 'silent)))
                     (let ((index
                            (2bit--read-index
                             source)))
                       (list
                        (gethash "a" index)
                        (gethash "xyz" index)
                        (gethash "missing"
                                 index 'absent)
                        (hash-table-count index)
                        (2bit-data-pos
                         source))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect!["OK (17 4096 absent 2 520)"],
    )
}

fn two_bit_block_collection_load_and_skip_consume_exact_word_counts() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_block_collection_load_and_skip_consume_exact_word_counts",
        r##"(let* ((file
                      (expand-file-name
                       "blocks.bin"
                       (getenv "TMPDIR")))
                     (source
                      (make-2bit-data
                       :source file
                       :pos 0)))
               (unwind-protect
                   (progn
                     (with-temp-buffer
                       (set-buffer-multibyte nil)
                       (dolist
                           (word
                            '(2 3 9 4 5
                              1 20 7
                              99))
                         (insert
                          (neomacs-2bit--word
                           word nil)))
                       (let
                           ((coding-system-for-write
                             'binary))
                         (write-region
                          (point-min)
                          (point-max)
                          file nil 'silent)))
                     (let ((blocks
                            (2bit--load-block-collection
                             source)))
                       (2bit--skip-block-collection
                        source)
                       (list
                        (2bit-block-collection-count
                         blocks)
                        (2bit-block-collection-starts
                         blocks)
                        (2bit-block-collection-sizes
                         blocks)
                        (2bit-data-pos source)
                        (2bit--read-word
                         source)
                        (2bit-data-pos
                         source))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect!["OK (2 (3 9) (4 5) 32 99 36)"],
    )
}

fn two_bit_open_decodes_little_and_big_endian_headers_and_public_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_open_decodes_little_and_big_endian_headers_and_public_metadata",
        r##"(let ((little
                    (expand-file-name
                     "little.2bit"
                     (getenv "TMPDIR")))
                   (big
                    (expand-file-name
                     "big.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      little nil)
                     (neomacs-2bit--write-fixture
                      big 'big)
                     (let ((little-data
                            (2bit-open little))
                           (big-data
                            (2bit-open big t)))
                       (list
                        (list
                         (2bit-data-signature
                          little-data)
                         (2bit-data-other-endian-p
                          little-data)
                         (2bit-data-version
                          little-data)
                         (2bit-data-sequence-count
                          little-data)
                         (2bit-data-masking
                          little-data)
                         (2bit-sequence-count
                          little-data)
                         (sort
                          (2bit-sequence-names
                           little-data)
                          #'string<))
                        (list
                         (2bit-data-signature
                          big-data)
                         (2bit-data-other-endian-p
                          big-data)
                         (2bit-data-version
                          big-data)
                         (2bit-data-sequence-count
                          big-data)
                         (2bit-data-masking
                          big-data)
                         (2bit-sequence-count
                          big)
                         (sort
                          (2bit-sequence-names
                           big)
                          #'string<)))))
                 (dolist (file (list little big))
                   (when (file-exists-p file)
                     (delete-file file)))))"##,
        expect![[
            r#"OK ((440477507 nil 0 2 nil 2 ("alpha" "beta")) (1126646042 t 0 2 t 2 ("alpha" "beta")))"#
        ]],
    )
}

fn two_bit_maybe_open_preserves_handles_and_opens_path_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "two_bit_maybe_open_preserves_handles_and_opens_path_values",
        r##"(let ((file
                    (expand-file-name
                     "maybe.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file)
                     (let* ((data
                             (2bit-open file))
                            (same
                             (2bit--maybe-open
                              data))
                            (fresh
                             (2bit--maybe-open
                              file)))
                       (list
                        (eq data same)
                        (eq data fresh)
                        (2bit-data-p fresh)
                        (2bit-data-source
                         fresh)
                        (2bit-data-sequence-count
                         fresh))))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"OK (t nil t "[ORACLE-TMPDIR]/maybe.2bit" 2)"#]],
    )
}

fn two_bit_open_rejects_a_missing_file_with_exact_signal_data() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_open_rejects_a_missing_file_with_exact_signal_data",
        r##"(2bit-open
              (expand-file-name
               "missing.2bit"
               (getenv "TMPDIR")))"##,
        expect![[r#"ERR (error "[ORACLE-TMPDIR]/missing.2bit does not exist")"#]],
    )
}

fn two_bit_open_rejects_an_invalid_signature_with_exact_decoded_value() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_open_rejects_an_invalid_signature_with_exact_decoded_value",
        r##"(let ((file
                    (expand-file-name
                     "bad-signature.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file nil 0 305419896)
                     (2bit-open file))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"ERR (error "Invalid 2bit signature: 305419896")"#]],
    )
}

fn two_bit_open_rejects_a_nonzero_version_with_exact_signal_data() -> ParityBatchCase {
    ParityBatchCase::signal(
        "two_bit_open_rejects_a_nonzero_version_with_exact_signal_data",
        r##"(let ((file
                    (expand-file-name
                     "bad-version.2bit"
                     (getenv "TMPDIR"))))
               (unwind-protect
                   (progn
                     (neomacs-2bit--write-fixture
                      file nil 7)
                     (2bit-open file))
                 (when (file-exists-p file)
                   (delete-file file))))"##,
        expect![[r#"ERR (error "7 is not a valid 2bit file version number")"#]],
    )
}

pub(super) fn binary_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        two_bit_public_constants_and_struct_defaults_match_the_pinned_release(),
        two_bit_relevant_blocks_observe_intersections_and_boundary_quirks(),
        two_bit_cursor_read_and_skip_preserve_binary_bytes_and_exact_position(),
        two_bit_word_helpers_decode_little_and_big_endian_values(),
        two_bit_read_word_and_words_advance_once_over_contiguous_data(),
        two_bit_index_reader_decodes_names_offsets_and_uses_worst_case_read_size(),
        two_bit_block_collection_load_and_skip_consume_exact_word_counts(),
        two_bit_open_decodes_little_and_big_endian_headers_and_public_metadata(),
        two_bit_maybe_open_preserves_handles_and_opens_path_values(),
        two_bit_open_rejects_a_missing_file_with_exact_signal_data(),
        two_bit_open_rejects_an_invalid_signature_with_exact_decoded_value(),
        two_bit_open_rejects_a_nonzero_version_with_exact_signal_data(),
    ]
}
