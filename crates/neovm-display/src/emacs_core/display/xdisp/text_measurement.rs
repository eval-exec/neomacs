//! Displayed text extents through the canonical offscreen row producer.
use super::*;
use crate::window::display::WindowDisplayContextExt;
use crate::window::{WindowDisplaySnapshot, WindowLayoutQueryOutcome, WindowLayoutQueryScope};

/// Named measurement inputs prevent mixing source bounds and fallback units.
#[derive(Clone, Copy)]
pub(super) struct TextMeasurement {
    pub frame: FrameId,
    pub window: crate::window::WindowId,
    pub buffer: BufferId,
    pub range: MeasuredRange,
    pub trim: bool,
    pub cell: TextCellPixels,
    pub columns: CharColumnWidth,
    pub edge: Option<RowEdge>,
    pub rewind_edge: Option<RowEdge>,
    pub explicit_x_limit: bool,
    pub(crate) y_limit: Option<f32>,
}

impl TextMeasurement {
    fn fallback(
        self,
        eval: &mut neovm_eval::emacs_core::eval::Context,
    ) -> Result<RegionTextMetrics, Flow> {
        region_text_metrics_with_display(
            eval,
            self.frame,
            self.buffer,
            self.range,
            self.trim,
            self.cell,
            self.columns,
            self.edge,
            self.rewind_edge,
            self.explicit_x_limit,
            self.y_limit,
        )
    }

    /// Only an absent frontend permits the startup/batch cell approximation.
    pub(super) fn measure(
        self,
        eval: &mut neovm_eval::emacs_core::eval::Context,
    ) -> Result<RegionTextMetrics, Flow> {
        let Self {
            frame,
            window,
            buffer,
            range,
            trim,
            cell,
            edge,
            rewind_edge,
            explicit_x_limit,
            y_limit,
            ..
        } = self;
        if y_limit == Some(0.0) {
            return Ok(RegionTextMetrics::EMPTY);
        }
        let source = eval
            .edit
            .buffers
            .get(buffer)
            .expect("resolved measurement buffer");
        let end_byte = if trim {
            let mut bytes = Vec::new();
            source.copy_emacs_byte_range_to(EmacsByteRange::new(range.from, range.to), &mut bytes);
            EmacsBytePos::new(
                range.from.get() + trim_window_text_to_non_empty_line_end(&bytes).len(),
            )
        } else {
            range.to
        };
        let start = source.emacs_byte_pos_to_lisp_char_pos(range.line_start);
        let from = source.emacs_byte_pos_to_lisp_char_pos(range.from);
        let end = source.emacs_byte_pos_to_lisp_char_pos(end_byte);
        let measure_width = edge.map(|edge| edge.x.ceil().max(0.0) as usize);
        let rewind_width = rewind_edge.map(|edge| edge.x.ceil().max(0.0) as usize);

        // When explicit `X-LIMIT` differs from `body_edge` and overlay strings
        // anchor directly at `FROM`, delegate to `region_text_metrics_with_display`
        // so `RewindBreakProbe` handles single-pass `FROM`-rewind without
        // double-counting overlay strings across a split `TextExtent` walk.
        let has_overlay_strings_at_from = !source.overlays().is_empty()
            && !collect_overlay_strings_at(source, range.from.get(), true).is_empty();
        if edge != rewind_edge && has_overlay_strings_at_from {
            return self.fallback(eval);
        }

        let finalize = |rows: usize,
                        mut width: i64,
                        height: i64,
                        origin_x: i64,
                        origin_is_newline: bool,
                        crossed_display_line: bool| {
            // GNU clamps `x` to `max_x` (`if (!NILP (x_limit)) if (x > max_x) x = max_x`,
            // src/xdisp.c:11985-11990) BEFORE subtracting `start_x` (src/xdisp.c:12052).
            if explicit_x_limit && let Some(edge) = edge {
                width = width.min(edge.x.ceil() as i64);
            }
            if !crossed_display_line && rows <= 1 && !(origin_is_newline && end > from) {
                width -= origin_x;
            }
            let height = y_limit.map_or(height as f32, |limit| (height as f32).min(limit));
            RegionTextMetrics {
                lines: rows,
                max_width: width as f32,
                height,
            }
        };

        // When `FROM` is mid-line and `measure_edge != rewind_edge`, mirror GNU
        // `window_text_pixel_size` (`src/xdisp.c:3507, 11867-11924`): rewind and
        // advance to `FROM` under `rewind_edge` (`body_edge`) first, then install
        // `measure_edge` (`X-LIMIT`) for `[FROM, TO)`.
        if from > start && edge != rewind_edge {
            let prefix_end =
                neovm_edit::buffer::LispCharPos1::new(from.as_i64().saturating_add(1)).min(end);
            let prefix = eval.query_window_layout_scope(
                frame,
                window,
                WindowLayoutQueryScope::TextExtent {
                    buffer,
                    start,
                    end: prefix_end,
                    width: rewind_width,
                    height: None,
                },
            );
            let Some(prefix_snapshot) = query_geometry(prefix)? else {
                return self.fallback(eval);
            };
            let origin = prefix_snapshot.point_for_buffer_pos(from);
            let origin_row = origin.as_ref().map_or(0, |point| point.row);
            let origin_x = origin.as_ref().map_or(0, |point| point.x);
            let origin_row_snapshot = prefix_snapshot
                .rows
                .iter()
                .find(|row| row.row == origin_row);
            let origin_row_height =
                origin_row_snapshot.map_or(cell.height.ceil().max(1.0) as i64, |row| row.height);
            let screen_line_start = origin_row_snapshot
                .and_then(|row| row.start_buffer_pos)
                .unwrap_or(start)
                .min(from);
            let source = eval.edit.buffers.get(buffer).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Measurement buffer was deleted")],
                )
            })?;
            let origin_is_newline = source
                .char_code_after_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(from))
                == Some('\n' as u32);

            // N1: Mirror GNU `src/xdisp.c:11920` (`if (FETCH_BYTE (start_bpos) == '\n')
            // it.current_x = 0`): advance past the newline at `FROM` instead of
            // re-walking `screen_line_start..from` under `measure_width` (which
            // would re-wrap the prefix and consume `y_limit` before reaching `FROM`).
            if origin_is_newline {
                if from >= end {
                    return Ok(finalize(1, 0, origin_row_height, origin_x, false, false));
                }
                let after_nl =
                    neovm_edit::buffer::LispCharPos1::new(from.as_i64().saturating_add(1));
                if after_nl >= end {
                    return Ok(finalize(1, 0, origin_row_height, origin_x, true, true));
                }
                let tail_query = eval.query_window_layout_scope(
                    frame,
                    window,
                    WindowLayoutQueryScope::TextExtent {
                        buffer,
                        start: after_nl,
                        end,
                        width: measure_width,
                        height: y_limit.and_then(|limit| {
                            std::num::NonZeroUsize::new(limit.ceil().max(1.0) as usize)
                        }),
                    },
                );
                let Some(tail_snapshot) = query_geometry(tail_query)? else {
                    return self.fallback(eval);
                };
                let source = eval.edit.buffers.get(buffer).ok_or_else(|| {
                    signal(
                        "error",
                        vec![Value::string("Measurement buffer was deleted")],
                    )
                })?;
                let mut rows = 1usize;
                let mut width = 0i64;
                let mut height = origin_row_height;
                for row in &tail_snapshot.rows {
                    if row.start_buffer_pos == Some(end)
                        && end > after_nl
                        && source.char_before_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(end))
                            == Some('\n')
                    {
                        break;
                    }
                    rows += 1;
                    height += row.height;
                    width = width.max(row.end_x);
                }
                return Ok(finalize(rows, width, height, origin_x, true, true));
            }

            if let Some(measure_edge) = edge
                && (origin_x as f32) >= measure_edge.x
            {
                if from >= end {
                    // N3: On a non-truncating window with `origin_x >= edge.x`,
                    // GNU `move_it_to` wraps past `last_visible_x` before stopping
                    // at `end == start`, clearing `start_x = 0` (`(4 . 1)`), while
                    // `truncate-lines = t` stays on row 0 (`(0 . 1)` / `(-1 . 1)`).
                    let crossed = !measure_edge.wrap.truncates();
                    return Ok(finalize(
                        1,
                        origin_x,
                        origin_row_height,
                        origin_x,
                        false,
                        crossed,
                    ));
                }
                if y_limit.is_some_and(|limit| limit < origin_row_height as f32) {
                    return Ok(RegionTextMetrics {
                        lines: 1,
                        max_width: 0.0,
                        height: y_limit.unwrap_or(0.0),
                    });
                }
                let tail_start = if measure_edge.wrap.truncates() {
                    let mut bytes = Vec::new();
                    source.copy_emacs_byte_range_to(
                        EmacsByteRange::new(range.from, end_byte),
                        &mut bytes,
                    );
                    match bytes.iter().position(|&b| b == b'\n') {
                        Some(offset) => {
                            let after_nl = EmacsBytePos::new(range.from.get() + offset + 1);
                            source.emacs_byte_pos_to_lisp_char_pos(after_nl)
                        }
                        None => {
                            return Ok(finalize(
                                1,
                                origin_x,
                                origin_row_height,
                                origin_x,
                                false,
                                false,
                            ));
                        }
                    }
                } else {
                    from
                };

                if tail_start >= end {
                    return Ok(finalize(
                        1,
                        origin_x,
                        origin_row_height,
                        origin_x,
                        false,
                        true,
                    ));
                }
                if let Some(limit) = y_limit
                    && limit <= origin_row_height as f32
                {
                    return Ok(finalize(
                        2,
                        origin_x,
                        origin_row_height,
                        origin_x,
                        false,
                        true,
                    ));
                }
                let remaining_height = y_limit.and_then(|limit| {
                    std::num::NonZeroUsize::new(
                        (limit - origin_row_height as f32).ceil().max(1.0) as usize
                    )
                });
                let tail_query = eval.query_window_layout_scope(
                    frame,
                    window,
                    WindowLayoutQueryScope::TextExtent {
                        buffer,
                        start: tail_start,
                        end,
                        width: measure_width,
                        height: remaining_height,
                    },
                );
                let Some(tail_snapshot) = query_geometry(tail_query)? else {
                    return self.fallback(eval);
                };
                let source = eval.edit.buffers.get(buffer).ok_or_else(|| {
                    signal(
                        "error",
                        vec![Value::string("Measurement buffer was deleted")],
                    )
                })?;
                let mut rows = 1usize;
                let mut width = origin_x;
                let mut height = origin_row_height;
                for row in &tail_snapshot.rows {
                    if row.start_buffer_pos == Some(end)
                        && end > tail_start
                        && source.char_before_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(end))
                            == Some('\n')
                    {
                        break;
                    }
                    rows += 1;
                    height += row.height;
                    width = width.max(row.end_x);
                }
                return Ok(finalize(rows, width, height, origin_x, false, true));
            }

            // `origin_x < measure_edge.x` (or unbounded `edge` / newline origin):
            // `FROM`'s screen line already starts at `screen_line_start` without
            // wrapping again before `FROM`, so run the measurement query from
            // `screen_line_start` under `measure_width`.
            let query = eval.query_window_layout_scope(
                frame,
                window,
                WindowLayoutQueryScope::TextExtent {
                    buffer,
                    start: screen_line_start,
                    end,
                    width: measure_width,
                    height: y_limit.and_then(|limit| {
                        std::num::NonZeroUsize::new(limit.ceil().max(1.0) as usize)
                    }),
                },
            );
            let Some(snapshot) = query_geometry(query)? else {
                return self.fallback(eval);
            };
            let origin = snapshot.point_for_buffer_pos(from);
            let origin_row = origin.as_ref().map_or(0, |point| point.row);
            let origin_x = origin.as_ref().map_or(origin_x, |point| point.x);
            let source = eval.edit.buffers.get(buffer).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Measurement buffer was deleted")],
                )
            })?;
            let mut rows = 0usize;
            let mut width = 0i64;
            let mut height = 0i64;
            for row in snapshot.rows.iter().filter(|row| row.row >= origin_row) {
                if row.start_buffer_pos == Some(end)
                    && end > from
                    && source.char_before_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(end))
                        == Some('\n')
                {
                    break;
                }
                rows += 1;
                height += row.height;
                let row_width = if origin_is_newline && row.row == origin_row {
                    row.end_x.saturating_sub(origin_x)
                } else {
                    row.end_x
                };
                width = width.max(row_width);
            }
            return Ok(finalize(
                rows,
                width,
                height,
                origin_x,
                origin_is_newline,
                false,
            ));
        }

        // The vertical budget begins at FROM's display row, after its wrapped
        // prefix. Include its first element so a wrap boundary is not mistaken
        // for the preceding row's artificial end-of-source insertion slot.
        let prefix_height = if y_limit.is_some() && from > start {
            let prefix = eval.query_window_layout_scope(
                frame,
                window,
                WindowLayoutQueryScope::TextExtent {
                    buffer,
                    start,
                    end: neovm_edit::buffer::LispCharPos1::new(from.as_i64().saturating_add(1))
                        .min(end),
                    width: measure_width,
                    height: None,
                },
            );
            let Some(snapshot) = query_geometry(prefix)? else {
                return self.fallback(eval);
            };
            snapshot
                .point_for_buffer_pos(from)
                .and_then(|origin| Some(origin.y.saturating_sub(snapshot.rows.first()?.y).max(0)))
                .unwrap_or(0)
        } else {
            0
        };
        let query = eval.query_window_layout_scope(
            frame,
            window,
            WindowLayoutQueryScope::TextExtent {
                buffer,
                start,
                end,
                width: measure_width,
                height: y_limit.and_then(|limit| {
                    std::num::NonZeroUsize::new(
                        (limit.ceil().max(1.0) as usize).saturating_add(prefix_height as usize),
                    )
                }),
            },
        );
        let Some(snapshot) = query_geometry(query)? else {
            return self.fallback(eval);
        };
        let origin = snapshot.point_for_buffer_pos(from);
        let origin_row = origin.as_ref().map_or(0, |point| point.row);
        let origin_x = origin.as_ref().map_or(0, |point| point.x);
        let source = eval.edit.buffers.get(buffer).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Measurement buffer was deleted")],
            )
        })?;
        let origin_is_newline = source
            .char_code_after_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(from))
            == Some('\n' as u32);
        let mut rows = 0;
        let mut width = 0i64;
        let mut height = 0i64;
        for row in snapshot.rows.iter().filter(|row| row.row >= origin_row) {
            // A trailing newline does not contribute the empty EOB row.
            if row.start_buffer_pos == Some(end)
                && end > from
                && source.char_before_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(end))
                    == Some('\n')
            {
                break;
            }
            rows += 1;
            height += row.height;
            // Source bounds constrain shaping before production. The row's
            // advance includes overlay strings, without counting insertion slots.
            // GNU resets current_x at a newline FROM, while preserving the
            // row's vertical extent and any display strings at that anchor.
            let row_width = if origin_is_newline && row.row == origin_row {
                row.end_x.saturating_sub(origin_x)
            } else {
                row.end_x
            };
            width = width.max(row_width);
        }
        Ok(finalize(
            rows,
            width,
            height,
            origin_x,
            origin_is_newline,
            false,
        ))
    }
}

/// Adapter absence is distinct from a failed canonical query.
///
/// A measurement that reenters an active layout query is *not* an error:
/// GNU's `buffer-text-pixel-size` is its own `display_string` walk and does
/// not consult the redisplay transaction at all. When the frame's engine is
/// owned by a redisplay callback (`(:eval (mode--line-format-right-align))`
/// measures `string-pixel-width` while the mode line is being laid out),
/// serve the measurement from the same display-aware scan used when no
/// frontend exists instead of failing redisplay (issue #715).
fn query_geometry(
    outcome: WindowLayoutQueryOutcome,
) -> Result<Option<WindowDisplaySnapshot>, Flow> {
    match outcome {
        WindowLayoutQueryOutcome::Unavailable | WindowLayoutQueryOutcome::LayoutBusy => Ok(None),
        WindowLayoutQueryOutcome::Failed(error) => {
            Err(signal("error", vec![Value::string(error.message())]))
        }
        WindowLayoutQueryOutcome::Ready(query) => {
            query.into_geometry().map(Some).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Text measurement produced no geometry")],
                )
            })
        }
    }
}
