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

/// Two spellings of one resource that are not equally wide still leave the
/// rest of the row -- text, styles and colors alike -- at matching columns.
#[test]
fn paired_environment_aligns_spellings_that_differ_in_width() {
    let mut gnu_bytes = b"F1 \x1b[33mnewcomment.el.gz\x1b[0m\x1b[36m   85%\x1b[0m".to_vec();
    let mut neo_bytes = b"F1 \x1b[33mnewcomment.el\x1b[0m\x1b[36m   85%\x1b[0m".to_vec();
    gnu_bytes.extend(std::iter::repeat_n(b'-', 28 - 19 - 6));
    neo_bytes.extend(std::iter::repeat_n(b'-', 28 - 16 - 6));
    let gnu = screen(1, 28, &gnu_bytes);
    let neo = screen(1, 28, &neo_bytes);
    let environment =
        PairedDisplayEnvironment::new().with_path_pair("newcomment.el.gz", "newcomment.el");

    let raw = compare_displays(gnu.screen(), neo.screen());
    let aligned = compare_displays_in_environment(gnu.screen(), neo.screen(), &environment);

    assert!(
        raw.unexpected()
            .iter()
            .any(|difference| matches!(difference, DisplayDifference::TextRow { .. })),
        "{raw:#?}"
    );
    assert!(aligned.is_satisfied(), "{aligned:#?}");
}

/// The alignment moves cells that follow a spelling, so a real difference
/// behind it is still a difference.
#[test]
fn paired_environment_still_rejects_a_different_row_behind_a_spelling() {
    let mut gnu_bytes = b"F1 \x1b[33mnewcomment.el.gz\x1b[0m\x1b[36m   85%\x1b[0m".to_vec();
    let mut neo_bytes = b"F1 \x1b[33mnewcomment.el\x1b[0m\x1b[36m   95%\x1b[0m".to_vec();
    gnu_bytes.extend(std::iter::repeat_n(b'-', 28 - 19 - 6));
    neo_bytes.extend(std::iter::repeat_n(b'-', 28 - 16 - 6));
    let gnu = screen(1, 28, &gnu_bytes);
    let neo = screen(1, 28, &neo_bytes);
    let environment =
        PairedDisplayEnvironment::new().with_path_pair("newcomment.el.gz", "newcomment.el");

    let aligned = compare_displays_in_environment(gnu.screen(), neo.screen(), &environment);

    assert!(
        aligned
            .unexpected()
            .iter()
            .any(|difference| matches!(difference, DisplayDifference::TextRow { .. })),
        "{aligned:#?}"
    );
}

/// A harness-minted session temporary root covers the name the editor minted
/// inside it, which no other session can spell the same way.
#[test]
fn paired_environment_absorbs_the_name_minted_in_a_session_scratch_root() {
    let gnu = screen(1, 60, b"/tmp/gnu-root/buffer-content-AAA1 alpha");
    let neo = screen(1, 60, b"/tmp/neo-root/buffer-content-BBB2 alpha");
    let scratch_root =
        PairedDisplayEnvironment::new().with_scratch_root_pair("/tmp/gnu-root", "/tmp/neo-root");
    let exact = PairedDisplayEnvironment::new().with_path_pair("/tmp/gnu-root", "/tmp/neo-root");

    let absorbed = compare_displays_in_environment(gnu.screen(), neo.screen(), &scratch_root);
    let exact_only = compare_displays_in_environment(gnu.screen(), neo.screen(), &exact);

    assert!(absorbed.is_satisfied(), "{absorbed:#?}");
    assert!(
        !exact_only.is_satisfied(),
        "an exactly declared root leaves the minted name compared: {exact_only:#?}"
    );
}

/// A declared resource is matched on its own terms, so a broader spelling
/// declared alongside it never hides the part that reaches further.
#[test]
fn paired_environment_prefers_the_declared_spelling_that_reaches_furthest() {
    let gnu = screen(1, 70, b"file /tmp/gnu-root/scratch-AA.txt done");
    let neo = screen(1, 70, b"file /tmp/neo-root/scratch-BB.txt done");

    for environment in [
        PairedDisplayEnvironment::new()
            .with_path_pair("/tmp/gnu-root", "/tmp/neo-root")
            .with_path_pair(
                "/tmp/gnu-root/scratch-AA.txt",
                "/tmp/neo-root/scratch-BB.txt",
            ),
        PairedDisplayEnvironment::new()
            .with_path_pair(
                "/tmp/gnu-root/scratch-AA.txt",
                "/tmp/neo-root/scratch-BB.txt",
            )
            .with_path_pair("/tmp/gnu-root", "/tmp/neo-root"),
    ] {
        let report = compare_displays_in_environment(gnu.screen(), neo.screen(), &environment);
        assert!(report.is_satisfied(), "{report:#?}");
    }
}

/// A cell an editor never painted and a cell holding a blank display the same
/// thing, so a row is not a difference just because one editor wrote its
/// blanks and the other left them alone.
#[test]
fn paired_environment_treats_unpainted_cells_as_the_blanks_they_show() {
    let gnu = screen(1, 8, b"a\x1b[1;6Hb");
    let neo = screen(1, 8, b"a    b");

    let report = compare_displays(gnu.screen(), neo.screen());

    assert!(report.is_satisfied(), "{report:#?}");
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
