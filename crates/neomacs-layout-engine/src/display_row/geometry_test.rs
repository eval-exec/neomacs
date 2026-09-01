use super::*;
use crate::window_output::{
    DisplayTextRowBegin, DisplayTextRowGeometryTransition, DisplayTextRowMetrics,
};

#[test]
fn current_display_row_metrics_tracks_glyph_extents_and_overflow() {
    let mut metrics = CurrentDisplayRowMetrics::new(16.0, 12.0);

    metrics.include_glyph(24.0, 18.0);

    assert_eq!(metrics.height(), 24.0);
    assert_eq!(metrics.ascent(), 18.0);
    assert_eq!(metrics.height_delta_from_default(16.0), 8.0);
    assert_eq!(
        metrics.finish_current_row(7.0),
        DisplayTextRowMetrics {
            y: 7.0,
            height: 24.0,
            ascent: 18.0,
        }
    );
}

#[test]
fn current_display_row_metrics_builds_next_row_vertical_delta() {
    let mut metrics = CurrentDisplayRowMetrics::new(16.0, 12.0);
    metrics.include_glyph(24.0, 18.0);

    assert_eq!(metrics.next_row_vertical_delta(16.0, 3.0), 11.0);
}

#[test]
fn current_display_row_metrics_resets_to_default_extents() {
    let mut metrics = CurrentDisplayRowMetrics::new(16.0, 12.0);
    metrics.include_glyph(24.0, 18.0);

    metrics.reset(14.0, 10.0);

    assert_eq!(metrics.height(), 14.0);
    assert_eq!(metrics.ascent(), 10.0);
    assert_eq!(metrics.height_delta_from_default(16.0), -2.0);
}

#[test]
fn content_only_short_row_advances_by_its_finished_height() {
    let mut metrics = CurrentDisplayRowMetrics::new(3.0, 2.0);

    let advance = metrics.finish_and_advance_to_next_row(CurrentDisplayRowAdvance {
        y: 0.0,
        next_row: 1,
        text_y: 0.0,
        row_extra_y: 0.0,
        default_height: 16.0,
        default_ascent: 12.0,
        kind: DisplayRowAdvanceKind::LineBreak { line_spacing: 0.0 },
    });

    assert_eq!(advance.finished.height, 3.0);
    assert_eq!(advance.row_extra_y, -13.0);
    assert_eq!(advance.next_y, 3.0);
    assert_eq!(metrics.height(), 16.0, "the next row resets to defaults");
}

#[test]
fn current_display_row_metrics_finishes_row_and_resets_to_default_extents() {
    let mut metrics = CurrentDisplayRowMetrics::new(16.0, 12.0);
    metrics.include_glyph(24.0, 18.0);

    let finished = metrics.finish_and_reset(7.0, 14.0, 10.0);

    assert_eq!(
        finished,
        DisplayTextRowMetrics {
            y: 7.0,
            height: 24.0,
            ascent: 18.0,
        }
    );
    assert_eq!(metrics.height(), 14.0);
    assert_eq!(metrics.ascent(), 10.0);
}

#[test]
fn current_display_row_metrics_finishes_current_row_without_resetting_extents() {
    let mut metrics = CurrentDisplayRowMetrics::new(16.0, 12.0);
    metrics.include_glyph(24.0, 18.0);

    let finished = metrics.finish_current_row(7.0);

    assert_eq!(
        finished,
        DisplayTextRowMetrics {
            y: 7.0,
            height: 24.0,
            ascent: 18.0,
        }
    );
    assert_eq!(metrics.height(), 24.0);
    assert_eq!(metrics.ascent(), 18.0);
}

#[test]
fn current_display_row_metrics_advances_to_next_row_from_finished_extents() {
    let mut metrics = CurrentDisplayRowMetrics::new(16.0, 12.0);
    metrics.include_glyph(24.0, 18.0);

    let advance = metrics.finish_and_advance_to_next_row(CurrentDisplayRowAdvance {
        y: 7.0,
        next_row: 3,
        text_y: 10.0,
        row_extra_y: 2.0,
        default_height: 16.0,
        default_ascent: 12.0,
        kind: DisplayRowAdvanceKind::LineBreak { line_spacing: 3.0 },
    });

    assert_eq!(
        advance,
        DisplayRowAdvance {
            finished: DisplayTextRowMetrics {
                y: 7.0,
                height: 24.0,
                ascent: 18.0,
            },
            next_y: 10.0 + 3.0 * 16.0 + 13.0,
            row_extra_y: 13.0,
            next_height: 16.0,
            next_ascent: 12.0,
        }
    );
    assert_eq!(metrics.height(), 16.0);
    assert_eq!(metrics.ascent(), 12.0);
}

#[test]
fn current_display_row_metrics_advances_visual_wrap_without_line_spacing() {
    let mut metrics = CurrentDisplayRowMetrics::new(16.0, 12.0);
    metrics.include_glyph(24.0, 18.0);

    let advance = metrics.finish_and_advance_to_next_row(CurrentDisplayRowAdvance {
        y: 7.0,
        next_row: 2,
        text_y: 10.0,
        row_extra_y: 2.0,
        default_height: 16.0,
        default_ascent: 12.0,
        kind: DisplayRowAdvanceKind::VisualWrap,
    });

    assert_eq!(advance.row_extra_y, 10.0);
    assert_eq!(advance.next_y, 10.0 + 2.0 * 16.0 + 10.0);
    assert_eq!(metrics.height(), 16.0);
    assert_eq!(metrics.ascent(), 12.0);
}

#[test]
fn display_row_geometry_cursor_advances_row_position_and_resets_metrics() {
    let mut cursor = DisplayRowGeometryCursor::from_state(DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    });

    let hit_row = cursor.hit_row(11, 22);
    assert_eq!(hit_row.y_start, 42.0);
    assert_eq!(hit_row.y_end, 66.0);
    assert_eq!(hit_row.charpos_start, 11);
    assert_eq!(hit_row.charpos_end, 22);

    let finished = cursor.finish_and_advance_to_next_row(
        DisplayRowGeometryDefaults {
            text_y: 10.0,
            height: 16.0,
            ascent: 12.0,
        },
        DisplayRowAdvanceKind::LineBreak { line_spacing: 4.0 },
    );

    assert_eq!(
        finished,
        DisplayTextRowMetrics {
            y: 42.0,
            height: 24.0,
            ascent: 18.0,
        }
    );
    assert_eq!(
        cursor.state(),
        DisplayRowGeometryState {
            row: 3,
            y: 10.0 + 3.0 * 16.0 + 15.0,
            row_extra_y: 15.0,
            height: 16.0,
            ascent: 12.0,
        }
    );
    assert_eq!(
        cursor.display_text_row_begin(5, 7, 13.0, LayoutCharPos0::new(21)),
        DisplayTextRowBegin {
            display_row_index: 8,
            row: 3,
            col: 7,
            y: 10.0 + 3.0 * 16.0 + 15.0,
            x: 13.0,
            start_charpos: LayoutCharPos0::new(21),
        }
    );
}

#[test]
fn display_row_geometry_cursor_finishes_current_row_without_advancing() {
    let cursor = DisplayRowGeometryCursor::from_state(DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    });

    assert_eq!(
        cursor.finish_current_row(),
        DisplayTextRowMetrics {
            y: 42.0,
            height: 24.0,
            ascent: 18.0,
        }
    );
    assert_eq!(
        cursor.state(),
        DisplayRowGeometryState {
            row: 2,
            y: 42.0,
            row_extra_y: 3.0,
            height: 24.0,
            ascent: 18.0,
        }
    );
}

#[test]
fn display_row_geometry_state_builds_cursor_after_row_y_adjustment() {
    let geometry = DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    };

    let cursor = geometry.with_row_y(48.0).cursor();
    let hit_row = cursor.hit_row(11, 22);

    assert_eq!(hit_row.y_start, 48.0);
    assert_eq!(hit_row.y_end, 72.0);
    assert_eq!(hit_row.charpos_start, 11);
    assert_eq!(hit_row.charpos_end, 22);
    assert_eq!(
        cursor.finish_current_row(),
        DisplayTextRowMetrics {
            y: 48.0,
            height: 24.0,
            ascent: 18.0,
        }
    );
}

#[test]
fn display_row_geometry_state_constructor_groups_current_row_fields() {
    assert_eq!(
        DisplayRowGeometryState::new(3, 40.0, 7.0, 18.0, 13.0),
        DisplayRowGeometryState {
            row: 3,
            y: 40.0,
            row_extra_y: 7.0,
            height: 18.0,
            ascent: 13.0,
        }
    );
}

#[test]
fn display_row_geometry_state_exposes_current_row_position_by_name() {
    let geometry = DisplayRowGeometryState::new(3, 40.0, 7.0, 18.0, 13.0);

    assert_eq!(geometry.row(), 3);
    assert_eq!(geometry.y(), 40.0);
}

#[test]
fn display_row_geometry_state_can_be_mutated_directly() {
    let mut geometry = DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    };

    geometry.row += 1;
    geometry.y += 16.0;
    geometry.row_extra_y += 5.0;
    geometry.include_row_extents(32.0, 20.0);

    assert_eq!(
        geometry,
        DisplayRowGeometryState {
            row: 3,
            y: 58.0,
            row_extra_y: 8.0,
            height: 32.0,
            ascent: 20.0,
        }
    );
}

#[test]
fn display_row_geometry_state_include_glyph_vertical_metrics_by_name() {
    let mut geometry = DisplayRowGeometryState {
        row: 4,
        y: 80.0,
        row_extra_y: 9.0,
        height: 16.0,
        ascent: 12.0,
    };

    geometry.include_glyph_vertical_metrics(24.0, 18.0);

    assert_eq!(geometry.row, 4);
    assert_eq!(geometry.y, 80.0);
    assert_eq!(geometry.row_extra_y, 9.0);
    assert_eq!(geometry.height, 24.0);
    assert_eq!(geometry.ascent, 18.0);
}

#[test]
fn display_row_geometry_state_include_row_extents_by_name() {
    let mut geometry = DisplayRowGeometryState {
        row: 4,
        y: 80.0,
        row_extra_y: 9.0,
        height: 16.0,
        ascent: 12.0,
    };

    geometry.include_row_extents(24.0, 24.0);

    assert_eq!(geometry.row, 4);
    assert_eq!(geometry.y, 80.0);
    assert_eq!(geometry.row_extra_y, 9.0);
    assert_eq!(geometry.height, 24.0);
    assert_eq!(geometry.ascent, 24.0);
}

#[test]
fn display_row_geometry_state_reports_current_row_visibility_by_limit() {
    let geometry = DisplayRowGeometryState {
        row: 4,
        y: 80.0,
        row_extra_y: 9.0,
        height: 24.0,
        ascent: 18.0,
    };

    assert!(geometry.current_row_is_visible(DisplayRowVisibilityLimit {
        max_rows: 5,
        bottom_y: 104.0,
    }));
    assert!(!geometry.current_row_is_visible(DisplayRowVisibilityLimit {
        max_rows: 4,
        bottom_y: 104.0,
    }));
    assert!(!geometry.current_row_is_visible(DisplayRowVisibilityLimit {
        max_rows: 5,
        bottom_y: 103.9,
    }));
}

#[test]
fn display_row_geometry_state_records_current_row_y() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let mut row_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);

    geometry.record_current_row_y(&mut row_y_positions);

    assert_eq!(row_y_positions.recorded(), &[8.0, 69.0]);
}

#[test]
fn display_row_geometry_state_builds_row_y_fallback_from_current_extra_y() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };

    let fallback = geometry.row_y_fallback(10.0, 16.0);

    assert_eq!(
        fallback,
        DisplayRowYFallback {
            text_y: 10.0,
            default_height: 16.0,
            row_extra_y: 11.0,
        }
    );
}

#[test]
fn display_row_geometry_state_resolves_recorded_current_row_y() {
    let geometry = DisplayRowGeometryState {
        row: 2,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let mut row_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    row_y_positions.record(1, 25.0);
    row_y_positions.record(2, 44.0);

    assert_eq!(geometry.current_row_y(&row_y_positions, 10.0, 16.0), 44.0);
}

#[test]
fn display_row_geometry_state_resolves_any_row_y_with_current_fallback() {
    let geometry = DisplayRowGeometryState {
        row: 2,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let mut row_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    row_y_positions.record(1, 25.0);

    assert_eq!(geometry.row_y(1, &row_y_positions, 10.0, 16.0), 25.0);
    assert_eq!(geometry.row_y(3, &row_y_positions, 10.0, 16.0), 69.0);
}

#[test]
fn display_row_geometry_state_resolves_row_limit_positions() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let limit = DisplayRowLimit { max_rows: 5 };

    assert!(geometry.is_within_row_limit(limit));
    assert_eq!(geometry.rendered_row_count(limit), 3);
    assert_eq!(geometry.first_row_below_current(limit), 4);

    let exhausted = DisplayRowLimit { max_rows: 3 };
    assert!(!geometry.is_within_row_limit(exhausted));
    assert_eq!(geometry.rendered_row_count(exhausted), 3);
    assert_eq!(geometry.first_row_below_current(exhausted), 3);
}

#[test]
fn display_row_flags_mark_and_query_typed_row_flags() {
    let geometry = DisplayRowGeometryState {
        row: 2,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let exhausted = DisplayRowGeometryState {
        row: 4,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let limit = DisplayRowLimit { max_rows: 4 };
    let mut flags = DisplayRowFlags::new(4);

    geometry.mark_current_row_flag_kind(&mut flags, DisplayRowFlagKind::Truncated, limit);
    geometry.mark_current_row_flag_kind(&mut flags, DisplayRowFlagKind::Continued, limit);
    exhausted.mark_current_row_flag_kind(&mut flags, DisplayRowFlagKind::Continuation, limit);

    assert!(flags.is_set(2, DisplayRowFlagKind::Truncated));
    assert!(flags.is_set(2, DisplayRowFlagKind::Continued));
    assert!(!flags.is_set(2, DisplayRowFlagKind::Continuation));
    assert!(!flags.is_set(4, DisplayRowFlagKind::Truncated));
}

#[test]
fn display_row_geometry_state_builds_typed_row_markers() {
    let geometry = DisplayRowGeometryState {
        row: 2,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let next_row_geometry = DisplayRowGeometryState {
        row: 3,
        y: 85.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };

    let current_marker = geometry.current_row_marker();
    let next_marker = geometry.next_row_marker();

    assert_eq!(current_marker, DisplayRowMarker::Row(2));
    assert_eq!(next_marker, DisplayRowMarker::Row(3));
    assert!(current_marker.is_active_on(&geometry));
    assert!(!current_marker.is_active_on(&next_row_geometry));
    assert!(next_marker.is_active_on(&next_row_geometry));
    assert!(!DisplayRowMarker::Inactive.is_active_on(&geometry));
}

#[test]
fn display_row_scoped_value_tracks_value_by_owning_row() {
    let geometry = DisplayRowGeometryState {
        row: 2,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let next_row_geometry = DisplayRowGeometryState {
        row: 3,
        y: 85.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let mut scoped_value = DisplayRowScopedValue::inactive();

    assert_eq!(scoped_value.value(), None);
    assert_eq!(scoped_value.value_on(&geometry), None);

    scoped_value.activate(geometry.current_row_marker(), "extend-face");

    assert_eq!(scoped_value.value(), Some(&"extend-face"));
    assert_eq!(scoped_value.value_on(&geometry), Some(&"extend-face"));
    assert_eq!(scoped_value.value_on(&next_row_geometry), None);

    scoped_value.clear();

    assert_eq!(scoped_value.value(), None);
}

#[test]
fn display_row_geometry_state_builds_row_scoped_start_marker() {
    let geometry = DisplayRowGeometryState {
        row: 2,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };
    let other_row_geometry = DisplayRowGeometryState {
        row: 3,
        y: 85.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };

    let marker = geometry.start_marker_at_x(42.5);

    assert_eq!(
        marker,
        DisplayRowStartMarker::Active {
            row: DisplayRowMarker::Row(2),
            x: 42.5,
        }
    );
    assert_eq!(marker.x_on(&geometry), Some(42.5));
    assert_eq!(marker.x_on(&other_row_geometry), None);
    assert!(marker.is_active());
    assert!(!DisplayRowStartMarker::Inactive.is_active());
}

#[test]
fn display_row_geometry_state_builds_text_position_from_current_row() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };

    assert_eq!(
        geometry.text_position(42.0, 91, 7),
        DisplayRowTextPosition {
            x: 42.0,
            y: 69.0,
            byte_idx: 91,
            col: 7,
            row: 3,
        }
    );
}

#[test]
fn display_row_geometry_state_builds_row_metrics_snapshot() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 24.0,
        ascent: 18.0,
    };

    let snapshot = geometry.row_metrics_snapshot(5);

    assert_eq!(snapshot.row(), 8);
    assert_eq!(snapshot.pixel_y(), 69.0);
    assert_eq!(snapshot.height(), 24.0);
    assert_eq!(snapshot.ascent(), 18.0);
}

#[test]
fn display_row_geometry_state_clamps_row_metrics_snapshot_extents() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 0.0,
        ascent: 7.0,
    };

    let snapshot = geometry.row_metrics_snapshot(5);

    assert_eq!(snapshot.height(), 1.0);
    assert_eq!(snapshot.ascent(), 1.0);
}

#[test]
fn display_row_geometry_state_builds_display_text_row_begin() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };

    let begin = geometry.display_text_row_begin(5, 7, 13.0, LayoutCharPos0::new(21));

    assert_eq!(
        begin,
        DisplayTextRowBegin {
            display_row_index: 8,
            row: 3,
            col: 7,
            y: 69.0,
            x: 13.0,
            start_charpos: LayoutCharPos0::new(21),
        }
    );
}

#[test]
fn display_row_geometry_state_resolves_glyph_y_with_offset() {
    let geometry = DisplayRowGeometryState {
        row: 3,
        y: 69.0,
        row_extra_y: 11.0,
        height: 16.0,
        ascent: 12.0,
    };

    assert_eq!(geometry.glyph_y(2.5), 71.5);
}

#[test]
fn display_row_y_positions_preserve_recorded_rows_and_fallback_by_geometry() {
    let mut positions = DisplayRowYPositions::with_first_row(10.0, 16.0);
    positions.record(1, 30.0);

    assert_eq!(
        positions.y_for_row(
            0,
            DisplayRowYFallback {
                text_y: 10.0,
                default_height: 16.0,
                row_extra_y: 9.0,
            }
        ),
        10.0
    );
    assert_eq!(
        positions.y_for_row(
            1,
            DisplayRowYFallback {
                text_y: 10.0,
                default_height: 16.0,
                row_extra_y: 9.0,
            }
        ),
        30.0
    );
    assert_eq!(
        positions.y_for_row(
            3,
            DisplayRowYFallback {
                text_y: 10.0,
                default_height: 16.0,
                row_extra_y: 9.0,
            }
        ),
        67.0
    );
}

#[test]
fn display_row_y_positions_expose_recording_target_without_engine_vec_access() {
    let mut positions = DisplayRowYPositions::with_first_row(10.0, 16.0);
    {
        let recording = positions.recording();
        let DisplayRowYRecording::RowYPositions(raw) = recording else {
            panic!("expected row-y recording target");
        };
        raw.push(30.0);
    }

    assert_eq!(positions.recorded(), &[10.0, 30.0]);
}

#[test]
fn display_row_geometry_cursor_finishes_and_builds_next_display_text_row_begin() {
    let mut cursor = DisplayRowGeometryCursor::from_state(DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    });

    let transition = cursor.finish_and_begin_next_display_text_row(
        DisplayRowGeometryDefaults {
            text_y: 10.0,
            height: 16.0,
            ascent: 12.0,
        },
        DisplayRowAdvanceKind::LineBreak { line_spacing: 4.0 },
        5,
        7,
        13.0,
        LayoutCharPos0::new(21),
    );

    assert_eq!(
        transition,
        DisplayTextRowGeometryTransition {
            finished_row: DisplayTextRowMetrics {
                y: 42.0,
                height: 24.0,
                ascent: 18.0,
            },
            begin_row: DisplayTextRowBegin {
                display_row_index: 8,
                row: 3,
                col: 7,
                y: 10.0 + 3.0 * 16.0 + 15.0,
                x: 13.0,
                start_charpos: LayoutCharPos0::new(21),
            },
        }
    );
    assert_eq!(
        cursor.state(),
        DisplayRowGeometryState {
            row: 3,
            y: 10.0 + 3.0 * 16.0 + 15.0,
            row_extra_y: 15.0,
            height: 16.0,
            ascent: 12.0,
        }
    );
}

#[test]
fn display_row_geometry_state_can_finish_boundary_and_record_hit_row() {
    let mut geometry = DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    };
    let mut row_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    let mut hit_rows = Vec::new();

    let transition = geometry.finish_boundary_and_record_hit(
        DisplayRowBoundaryTarget::visual_wrap(
            DisplayRowHitRange {
                charpos_start: 11,
                charpos_end: 22,
            },
            DisplayRowGeometryDefaults {
                text_y: 10.0,
                height: 16.0,
                ascent: 12.0,
            },
            5,
            7,
            13.0,
            row_y_positions.recording(),
        ),
        &mut hit_rows,
    );

    assert_eq!(geometry.row, 3);
    assert_eq!(geometry.y, 10.0 + 3.0 * 16.0 + 11.0);
    assert_eq!(geometry.row_extra_y, 11.0);
    assert_eq!(geometry.height, 16.0);
    assert_eq!(geometry.ascent, 12.0);
    assert_eq!(hit_rows.len(), 1);
    assert_eq!(hit_rows[0].y_start, 42.0);
    assert_eq!(hit_rows[0].y_end, 66.0);
    assert_eq!(hit_rows[0].charpos_start, 11);
    assert_eq!(hit_rows[0].charpos_end, 22);
    assert_eq!(
        transition.begin_row,
        DisplayTextRowBegin {
            display_row_index: 8,
            row: 3,
            col: 7,
            y: 10.0 + 3.0 * 16.0 + 11.0,
            x: 13.0,
            start_charpos: LayoutCharPos0::new(22),
        }
    );
    assert_eq!(row_y_positions.recorded(), &[8.0, 10.0 + 3.0 * 16.0 + 11.0]);
}

#[test]
fn display_row_geometry_transition_target_groups_truncation_transition_and_commit_inputs() {
    let mut geometry = DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    };
    let mut row_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    let mut hit_rows = Vec::new();

    let transition = geometry.finish_boundary_and_record_hit(
        DisplayRowBoundaryTarget::new(
            DisplayRowHitRange {
                charpos_start: 0,
                charpos_end: 0,
            },
            DisplayRowGeometryTransitionTarget::truncation(
                DisplayRowGeometryDefaults {
                    text_y: 10.0,
                    height: 16.0,
                    ascent: 12.0,
                },
                5,
                7,
                13.0,
                row_y_positions.recording(),
            ),
        ),
        &mut hit_rows,
    );

    assert_eq!(
        transition,
        DisplayTextRowGeometryTransition {
            finished_row: DisplayTextRowMetrics {
                y: 42.0,
                height: 24.0,
                ascent: 18.0,
            },
            begin_row: DisplayTextRowBegin {
                display_row_index: 8,
                row: 3,
                col: 7,
                y: 10.0 + 3.0 * 16.0 + 11.0,
                x: 13.0,
                start_charpos: LayoutCharPos0::new(0),
            },
        }
    );
    assert_eq!(geometry.row, 3);
    assert_eq!(geometry.y, 10.0 + 3.0 * 16.0 + 11.0);
    assert_eq!(geometry.row_extra_y, 11.0);
    assert_eq!(geometry.height, 16.0);
    assert_eq!(geometry.ascent, 12.0);
    assert_eq!(row_y_positions.recorded(), &[8.0, 10.0 + 3.0 * 16.0 + 11.0]);
}

#[test]
fn display_row_geometry_transition_target_line_break_constructor_sets_kind() {
    let mut geometry = DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    };
    let mut row_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    let mut hit_rows = Vec::new();

    let transition = geometry.finish_boundary_and_record_hit(
        DisplayRowBoundaryTarget::new(
            DisplayRowHitRange {
                charpos_start: 0,
                charpos_end: 0,
            },
            DisplayRowGeometryTransitionTarget::line_break(
                DisplayRowGeometryDefaults {
                    text_y: 10.0,
                    height: 16.0,
                    ascent: 12.0,
                },
                5,
                7,
                13.0,
                4.0,
                row_y_positions.recording(),
            ),
        ),
        &mut hit_rows,
    );

    assert_eq!(
        transition.begin_row,
        DisplayTextRowBegin {
            display_row_index: 8,
            row: 3,
            col: 7,
            y: 10.0 + 3.0 * 16.0 + 15.0,
            x: 13.0,
            start_charpos: LayoutCharPos0::new(0),
        }
    );
    assert_eq!(geometry.row_extra_y, 15.0);
    assert_eq!(row_y_positions.recorded(), &[8.0, 10.0 + 3.0 * 16.0 + 15.0]);
}

#[test]
fn display_row_geometry_state_can_finish_boundary_without_row_y_recording() {
    let mut geometry = DisplayRowGeometryState {
        row: 2,
        y: 42.0,
        row_extra_y: 3.0,
        height: 24.0,
        ascent: 18.0,
    };

    let boundary = geometry.finish_boundary_in_place(DisplayRowBoundaryTarget::line_break(
        DisplayRowHitRange {
            charpos_start: 11,
            charpos_end: 22,
        },
        DisplayRowGeometryDefaults {
            text_y: 10.0,
            height: 16.0,
            ascent: 12.0,
        },
        5,
        7,
        13.0,
        2.0,
        DisplayRowYRecording::None,
    ));

    assert_eq!(boundary.hit_row.y_start, 42.0);
    assert_eq!(boundary.hit_row.y_end, 66.0);
    assert_eq!(geometry.row, 3);
    assert_eq!(geometry.y, 10.0 + 3.0 * 16.0 + 13.0);
    assert_eq!(geometry.row_extra_y, 13.0);
    assert_eq!(geometry.height, 16.0);
    assert_eq!(geometry.ascent, 12.0);
}

#[test]
fn display_row_boundary_transition_records_hit_row_and_returns_geometry_transition() {
    let boundary = DisplayRowBoundaryTransition {
        hit_row: HitRow {
            y_start: 42.0,
            y_end: 66.0,
            charpos_start: 11,
            charpos_end: 22,
        },
        transition: DisplayTextRowGeometryTransition {
            finished_row: DisplayTextRowMetrics {
                y: 42.0,
                height: 24.0,
                ascent: 18.0,
            },
            begin_row: DisplayTextRowBegin {
                display_row_index: 8,
                row: 3,
                col: 7,
                y: 69.0,
                x: 13.0,
                start_charpos: LayoutCharPos0::new(21),
            },
        },
    };
    let mut hit_rows = Vec::new();

    let transition = boundary.record_hit_row(&mut hit_rows);

    assert_eq!(hit_rows.len(), 1);
    assert_eq!(hit_rows[0].y_start, 42.0);
    assert_eq!(hit_rows[0].y_end, 66.0);
    assert_eq!(hit_rows[0].charpos_start, 11);
    assert_eq!(hit_rows[0].charpos_end, 22);
    assert_eq!(
        transition,
        DisplayTextRowGeometryTransition {
            finished_row: DisplayTextRowMetrics {
                y: 42.0,
                height: 24.0,
                ascent: 18.0,
            },
            begin_row: DisplayTextRowBegin {
                display_row_index: 8,
                row: 3,
                col: 7,
                y: 69.0,
                x: 13.0,
                start_charpos: LayoutCharPos0::new(21),
            },
        }
    );
}

#[test]
fn display_row_geometry_defaults_constructor_groups_row_baseline_metrics() {
    assert_eq!(
        DisplayRowGeometryDefaults::new(10.0, 16.0, 12.0),
        DisplayRowGeometryDefaults {
            text_y: 10.0,
            height: 16.0,
            ascent: 12.0,
        }
    );
}

#[test]
fn display_row_geometry_defaults_build_initial_row_state() {
    let defaults = DisplayRowGeometryDefaults::new(10.0, 16.0, 12.0);

    assert_eq!(
        defaults.initial_state(),
        DisplayRowGeometryState::new(0, 10.0, 0.0, 16.0, 12.0)
    );
}

#[test]
fn display_row_geometry_defaults_build_row_y_fallback() {
    let defaults = DisplayRowGeometryDefaults::new(10.0, 16.0, 12.0);

    assert_eq!(
        defaults.row_y_fallback(9.0),
        DisplayRowYFallback {
            text_y: 10.0,
            default_height: 16.0,
            row_extra_y: 9.0,
        }
    );
}

#[test]
fn display_row_boundary_target_constructors_encode_boundary_kind_and_hit_range() {
    let defaults = DisplayRowGeometryDefaults {
        text_y: 10.0,
        height: 16.0,
        ascent: 12.0,
    };

    let mut line_break_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    let line_break = DisplayRowBoundaryTarget::line_break(
        DisplayRowHitRange {
            charpos_start: 11,
            charpos_end: 22,
        },
        defaults,
        5,
        7,
        13.0,
        4.0,
        line_break_y_positions.recording(),
    );
    let mut truncation_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    let truncation = DisplayRowBoundaryTarget::truncation(
        DisplayRowHitRange {
            charpos_start: 11,
            charpos_end: 22,
        },
        defaults,
        5,
        7,
        13.0,
        truncation_y_positions.recording(),
    );
    let mut visual_wrap_y_positions = DisplayRowYPositions::with_first_row(8.0, 16.0);
    let visual_wrap = DisplayRowBoundaryTarget::visual_wrap(
        DisplayRowHitRange {
            charpos_start: 11,
            charpos_end: 22,
        },
        defaults,
        5,
        7,
        13.0,
        visual_wrap_y_positions.recording(),
    );

    assert_eq!(line_break.hit_range.charpos_start, 11);
    assert_eq!(line_break.hit_range.charpos_end, 22);
    assert!(matches!(
        line_break.transition.kind,
        DisplayRowAdvanceKind::LineBreak { line_spacing: 4.0 }
    ));
    assert!(matches!(
        truncation.transition.kind,
        DisplayRowAdvanceKind::Truncation
    ));
    assert!(matches!(
        visual_wrap.transition.kind,
        DisplayRowAdvanceKind::VisualWrap
    ));
}
