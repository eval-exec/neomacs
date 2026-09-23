use super::*;

fn screen(rows: u16, cols: u16, bytes: &[u8]) -> vt100::Parser {
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(bytes);
    parser
}

#[test]
fn exact_display_rejects_different_terminal_geometry() {
    let gnu = screen(3, 8, b"same");
    let neo = screen(4, 8, b"same");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(report.unexpected().iter().any(|difference| matches!(
        difference,
        DisplayDifference::Geometry {
            gnu: DisplaySize {
                rows: 3,
                columns: 8
            },
            neomacs: DisplaySize {
                rows: 4,
                columns: 8
            }
        }
    )));
}

#[test]
fn exact_display_rejects_a_different_text_row() {
    let gnu = screen(2, 8, b"alpha");
    let neo = screen(2, 8, b"alpHa");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert_eq!(
        report.unexpected(),
        &[DisplayDifference::TextRow {
            row: TuiRow::absolute(0),
            gnu: "alpha".to_string(),
            neomacs: "alpHa".to_string(),
        }]
    );
}

#[test]
fn paired_environment_normalizes_only_its_declared_session_paths() {
    let gnu = screen(2, 80, b"Wrote /tmp/tui-home-gnu-ABC123/example.txt");
    let neo = screen(2, 80, b"Wrote /tmp/tui-home-neo-XYZ789/example.txt");
    let environment = PairedDisplayEnvironment::new()
        .with_path_pair("/tmp/tui-home-gnu-ABC123", "/tmp/tui-home-neo-XYZ789");

    let raw = compare_displays(gnu.screen(), neo.screen());
    let normalized = compare_displays_in_environment(gnu.screen(), neo.screen(), &environment);

    assert_eq!(raw.unexpected().len(), 1);
    assert!(normalized.is_satisfied(), "{normalized:#?}");
}

#[test]
fn exact_display_treats_written_and_unwritten_blank_cells_as_same_display() {
    let gnu = screen(1, 8, b"abc");
    let neo = screen(1, 8, b"abc   \x1b[1;4H");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(report.is_satisfied(), "{report:#?}");
}

#[test]
fn exact_display_includes_text_attributes_in_face_classes() {
    let gnu = screen(1, 4, b"\x1b[31mA\x1b[1mB");
    let neo = screen(1, 4, b"\x1b[32mAB");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(report.unexpected().iter().any(|difference| matches!(
        difference,
        DisplayDifference::StyleClass {
            cell: DisplayCell { row: 0, column: 1 },
            gnu_class: DisplayCell { row: 0, column: 1 },
            neomacs_class: DisplayCell { row: 0, column: 0 },
        }
    )));
}

#[test]
fn exact_display_requires_rgb_equality_beyond_style_topology() {
    let gnu = screen(1, 4, b"\x1b[38;2;255;0;0mAB\x1b[0m");
    let neo = screen(1, 4, b"\x1b[38;2;0;255;0mAB\x1b[0m");

    let report = compare_displays(gnu.screen(), neo.screen());
    let topology_only = compare_displays_with_color_contract(
        gnu.screen(),
        neo.screen(),
        DisplayColorContract::StyleTopology,
    );

    assert!(report.unexpected().iter().any(|difference| matches!(
        difference,
        DisplayDifference::Colors {
            cell: DisplayCell { row: 0, column: 0 },
            contract: DisplayColorContract::ResolvedRgb,
            gnu: DisplayCellColors {
                foreground: Some(DisplayColor::Rgb(255, 0, 0)),
                background: DisplayColor::Default,
            },
            neomacs: DisplayCellColors {
                foreground: Some(DisplayColor::Rgb(0, 255, 0)),
                background: DisplayColor::Default,
            },
        }
    )));
    assert!(topology_only.is_satisfied(), "{topology_only:#?}");
}

#[test]
fn resolved_rgb_equates_xterm_index_with_rgb_but_exact_terminal_values_do_not() {
    let indexed = screen(1, 2, b"\x1b[38;5;196mA\x1b[0m");
    let rgb = screen(1, 2, b"\x1b[38;2;255;0;0mA\x1b[0m");

    let resolved = compare_displays(indexed.screen(), rgb.screen());
    let terminal_values = compare_displays_with_color_contract(
        indexed.screen(),
        rgb.screen(),
        DisplayColorContract::ExactTerminalValues,
    );

    assert!(resolved.is_satisfied(), "{resolved:#?}");
    assert!(
        terminal_values
            .unexpected()
            .iter()
            .any(|difference| matches!(
                difference,
                DisplayDifference::Colors {
                    cell: DisplayCell { row: 0, column: 0 },
                    contract: DisplayColorContract::ExactTerminalValues,
                    gnu: DisplayCellColors {
                        foreground: Some(DisplayColor::Indexed(196)),
                        background: DisplayColor::Default,
                    },
                    neomacs: DisplayCellColors {
                        foreground: Some(DisplayColor::Rgb(255, 0, 0)),
                        background: DisplayColor::Default,
                    },
                }
            ))
    );
}

#[test]
fn exact_display_ignores_foreground_only_state_on_blank_cells() {
    // GNU's `tty_clear_end_of_line` clears while the current face's
    // foreground is still active.  A renderer may reset that foreground
    // first; with the default background the two blank remainders are
    // visually identical even though their terminal cells differ.
    let gnu = screen(1, 8, b"\x1b[31mabc\x1b[K");
    let neo = screen(1, 8, b"\x1b[31mabc\x1b[0m\x1b[K");

    let report = compare_displays(gnu.screen(), neo.screen());
    let terminal_values = compare_displays_with_color_contract(
        gnu.screen(),
        neo.screen(),
        DisplayColorContract::ExactTerminalValues,
    );

    assert!(report.is_satisfied(), "{report:#?}");
    assert!(
        terminal_values
            .unexpected()
            .iter()
            .any(|difference| matches!(
                difference,
                DisplayDifference::Colors {
                    cell: DisplayCell { row: 0, column: 3 },
                    contract: DisplayColorContract::ExactTerminalValues,
                    ..
                }
            ))
    );
}

#[test]
fn exact_display_retains_visible_background_state_on_blank_cells() {
    let gnu = screen(1, 8, b"\x1b[1;2H\x1b[31mabc\x1b[44m   ");
    let neo = screen(1, 8, b"\x1b[1;2H\x1b[32mabc\x1b[0m   ");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(report.unexpected().iter().any(|difference| matches!(
        difference,
        DisplayDifference::StyleClass {
            cell: DisplayCell { row: 0, column: 4 },
            ..
        }
    )));
}

#[test]
fn resolved_rgb_uses_resolved_colors_for_style_class_boundaries() {
    let indexed = screen(1, 3, b"\x1b[38;5;196mAB\x1b[0m");
    let mixed = screen(1, 3, b"\x1b[38;2;255;0;0mA\x1b[38;5;196mB\x1b[0m");

    let report = compare_displays(indexed.screen(), mixed.screen());

    assert!(report.is_satisfied(), "{report:#?}");
}

#[test]
fn exact_display_retains_visible_underline_state_on_blank_cells() {
    let gnu = screen(1, 8, b"\x1b[1;2H\x1b[31mabc\x1b[4m   ");
    let neo = screen(1, 8, b"\x1b[1;2H\x1b[32mabc\x1b[0m   ");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(report.unexpected().iter().any(|difference| matches!(
        difference,
        DisplayDifference::StyleClass {
            cell: DisplayCell { row: 0, column: 4 },
            ..
        }
    )));
}

#[test]
fn exact_display_rejects_different_soft_wrap_state() {
    let gnu = screen(2, 4, b"abcde");
    let neo = screen(2, 4, b"abcd\x1b[2;1He");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(report.unexpected().contains(&DisplayDifference::RowWrap {
        row: TuiRow::absolute(0),
        gnu: true,
        neomacs: false,
    }));
}

#[test]
fn exact_display_rejects_a_different_cursor_position() {
    let gnu = screen(2, 8, b"abc");
    let neo = screen(2, 8, b"abc\x1b[1;1H");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(
        report
            .unexpected()
            .contains(&DisplayDifference::CursorPosition {
                gnu: DisplayCell { row: 0, column: 3 },
                neomacs: DisplayCell { row: 0, column: 0 },
            })
    );
}

#[test]
fn exact_display_rejects_different_cursor_visibility() {
    let gnu = screen(2, 8, b"abc\x1b[?25l");
    let neo = screen(2, 8, b"abc");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(
        report
            .unexpected()
            .contains(&DisplayDifference::CursorVisibility {
                gnu: CursorVisibility::Hidden,
                neomacs: CursorVisibility::Visible,
            })
    );
}
