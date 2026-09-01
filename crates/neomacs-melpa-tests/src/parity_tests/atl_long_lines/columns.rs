use expect_test::expect;

use super::ParityBatchCase;

fn atl_long_lines_end_column_measures_ascii_empty_and_trailing_space_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_end_column_measures_ascii_empty_and_trailing_space_lines",
        r##"(with-temp-buffer
         (insert
          "\n"
          "alpha\n"
          "two trailing  \n")
         (goto-char
          (point-min))
         (let (columns)
           (dotimes
               (_ 3)
             (push
              (atl-long-lines--end-line-column)
              columns)
             (forward-line 1))
           (nreverse columns)))"##,
        expect!["OK (0 5 14)"],
    )
}

fn atl_long_lines_end_column_honors_buffer_local_tab_width_and_tab_stops() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_end_column_honors_buffer_local_tab_width_and_tab_stops",
        r##"(mapcar
         (lambda (width)
           (with-temp-buffer
             (setq-local
              tab-width
              width)
             (insert
              "a\tb\t")
             (goto-char
              (point-min))
             (list
              width
              (atl-long-lines--end-line-column))))
         '(2 4 8 16))"##,
        expect!["OK ((2 4) (4 8) (8 16) (16 32))"],
    )
}

fn atl_long_lines_end_column_counts_wide_combining_and_multilingual_characters() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_long_lines_end_column_counts_wide_combining_and_multilingual_characters",
        r##"(mapcar
         (lambda (text)
           (with-temp-buffer
             (insert text)
             (goto-char
              (point-min))
             (list
              text
              (length text)
              (string-width text)
              (atl-long-lines--end-line-column))))
         '("雪λ"
           "é"
           "🙂x"
           "日本語 abc"))"##,
        expect![[r#"OK (("雪λ" 2 3 3) ("é" 2 1 1) ("🙂x" 2 3 3) ("日本語 abc" 7 10 10))"#]],
    )
}

fn atl_long_lines_end_column_uses_the_current_logical_line_not_buffer_maximum() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_end_column_uses_the_current_logical_line_not_buffer_maximum",
        r##"(with-temp-buffer
         (insert
          "short\n"
          "this is the longest line in this fixture\n"
          "mid-size\n")
         (mapcar
          (lambda (line)
            (goto-char
             (point-min))
            (forward-line line)
            (list
             line
             (line-number-at-pos)
             (atl-long-lines--end-line-column)))
          '(2 0 1)))"##,
        expect!["OK ((2 3 8) (0 1 5) (1 2 40))"],
    )
}

fn atl_long_lines_end_column_respects_narrowing_at_a_partial_line_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_end_column_respects_narrowing_at_a_partial_line_boundary",
        r##"(with-temp-buffer
         (insert
          "prefix-ABCDEFGHIJ-suffix\nnext")
         (let ((start
                (+ (point-min) 7))
               (end
                (+ (point-min) 17)))
           (narrow-to-region
            start
            end)
           (goto-char
            (point-min))
           (list
            (buffer-string)
            (point-min)
            (point-max)
            (line-end-position)
            (atl-long-lines--end-line-column))))"##,
        expect![[r#"OK ("ABCDEFGHIJ" 8 18 18 10)"#]],
    )
}

fn atl_long_lines_end_column_preserves_point_mark_current_buffer_and_match_data() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_long_lines_end_column_preserves_point_mark_current_buffer_and_match_data",
        r##"(let ((original
                (current-buffer)))
         (with-temp-buffer
           (insert
            "alpha beta gamma")
           (goto-char 7)
           (set-mark 3)
           (string-match
            "\\(beta\\)"
            (buffer-string))
           (let ((point-before
                  (point))
                 (mark-before
                  (mark))
                 (match-before
                  (match-data))
                 (column
                  (atl-long-lines--end-line-column)))
             (list
              column
              (= (point)
                 point-before)
              (= (mark)
                 mark-before)
              (equal
               (match-data)
               match-before)
              (not
               (eq
                original
                (current-buffer)))))))"##,
        expect!["OK (16 t t t t)"],
    )
}

fn atl_long_lines_end_column_tracks_live_edits_to_the_same_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_end_column_tracks_live_edits_to_the_same_line",
        r##"(with-temp-buffer
         (insert "abc")
         (goto-char 2)
         (let ((initial
                (atl-long-lines--end-line-column)))
           (goto-char
            (point-max))
           (insert "\tZ")
           (let ((extended
                  (atl-long-lines--end-line-column)))
             (delete-region
              2
              4)
             (list
              initial
              extended
              (buffer-string)
              (atl-long-lines--end-line-column)))))"##,
        expect![[r#"OK (3 9 "a\11Z" 9)"#]],
    )
}

pub(super) fn columns_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_long_lines_end_column_measures_ascii_empty_and_trailing_space_lines(),
        atl_long_lines_end_column_honors_buffer_local_tab_width_and_tab_stops(),
        atl_long_lines_end_column_counts_wide_combining_and_multilingual_characters(),
        atl_long_lines_end_column_uses_the_current_logical_line_not_buffer_maximum(),
        atl_long_lines_end_column_respects_narrowing_at_a_partial_line_boundary(),
        atl_long_lines_end_column_preserves_point_mark_current_buffer_and_match_data(),
        atl_long_lines_end_column_tracks_live_edits_to_the_same_line(),
    ]
}
