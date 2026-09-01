use expect_test::expect;

use super::ParityBatchCase;

fn alda_history_append_text_preserves_order_newlines_and_clear_resets_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_history_append_text_preserves_order_newlines_and_clear_resets_state",
        r##"(let ((*alda-history* ""))
         (list
          (alda-history-append-text "piano:")
          *alda-history*
          (alda-history-append-text "  c d e")
          *alda-history*
          (alda-history-clear)
          *alda-history*))"##,
        expect![[r#"OK ("\npiano:" "\npiano:" "\npiano:\n  c d e" "\npiano:\n  c d e" "" "")"#]],
    )
}

fn alda_history_region_appends_plain_text_or_reports_empty_mark() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_history_region_appends_plain_text_or_reports_empty_mark",
        r##"(let ((*alda-history* "")
                messages)
         (cl-letf (((symbol-function 'message)
                    (lambda (format-string &rest args)
                      (push (apply #'format format-string args)
                            messages))))
           (with-temp-buffer
             (insert #("piano: c d e" 0 5 (face bold)))
             (list
              (alda-history-append-region 1 7)
              *alda-history*
              (alda-history-append-region 4 4)
              *alda-history*
              (nreverse messages)))))"##,
        expect![[r#"OK ("\npiano:" "\npiano:" #1=("no mark was set") "\npiano:" #1#)"#]],
    )
}

fn alda_history_buffer_line_and_block_use_real_buffer_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_history_buffer_line_and_block_use_real_buffer_boundaries",
        r##"(let ((*alda-history* "")
                calls)
         (cl-letf
             (((symbol-function 'alda-history-append-text)
               (lambda (text)
                 (push (list 'text text) calls)
                 'text))
              ((symbol-function 'alda-history-append-region)
               (lambda (start end)
                 (push
                  (list 'region start end
                        (buffer-substring-no-properties
                         start end))
                  calls)
                 'region)))
           (with-temp-buffer
             (insert "piano:\n  c d e\n\nviolin:\n  f g a\n")
             (goto-char 12)
             (list
              (alda-history-append-buffer)
              (alda-history-append-line)
              (alda-history-append-block)
              (nreverse calls)))))"##,
        expect![[
            r#"OK (text region region ((text "piano:\n  c d e\n\nviolin:\n  f g a\n") (region 8 15 "  c d e") (region 1 16 "piano:\n  c d e\n")))"#
        ]],
    )
}

fn alda_history_accumulates_context_then_playback_emits_complete_marker_score() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_history_accumulates_context_then_playback_emits_complete_marker_score",
        r##"(let ((*alda-history* "")
                calls)
         (cl-letf (((symbol-function 'alda-run-cmd)
                    (lambda (&rest args)
                      (push args calls)
                      'played)))
           (with-temp-buffer
             (insert "piano:\n  o4 c d e\n\nf g a b")
             (alda-history-append-region 1 21)
             (alda-play-region 23 (point-max))
             (list *alda-history*
                   (nreverse calls)))))"##,
        expect![[
            r#"OK ("\npiano:\n  o4 c d e\n\nf" (("play" "-F" "alda-mode-internal-marker" "--code" "\npiano:\n  o4 c d e\n\nf\n%alda-mode-internal-marker\n a b")))"#
        ]],
    )
}

pub(super) fn history_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alda_history_append_text_preserves_order_newlines_and_clear_resets_state(),
        alda_history_region_appends_plain_text_or_reports_empty_mark(),
        alda_history_buffer_line_and_block_use_real_buffer_boundaries(),
        alda_history_accumulates_context_then_playback_emits_complete_marker_score(),
    ]
}
