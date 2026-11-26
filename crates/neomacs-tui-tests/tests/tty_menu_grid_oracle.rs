#![cfg(unix)]
//! N4 (peer 6163): the TTY drop-down menu grid must match GNU.
//!
//! GNU draws a TTY menu with `tty_menu_activate`/`tty_menu_display`
//! (src/term.c) and `display_tty_menu_item` (src/xdisp.c):
//!
//! * the pane name is drawn one row above the first item, so a menu-bar
//!   drop-down marks its active heading on the menu-bar row ("Help >");
//! * every item name is padded to the widest name in the pane, the
//!   key-equivalent description follows after two spaces
//!   (src/keyboard.c:9093-9100), and the row is padded to the pane width;
//! * a separator is a run of dashes as wide as the pane.
//!
//! F10 on the Help heading exercises all three.  The reproduced defect was a
//! grid shifted one row down and two columns right, with no heading marker
//! and with separators rendered as the raw "--" label.

use crate::support::*;
use std::time::Duration;

fn help_menu_ready(grid: &[String]) -> bool {
    grid.iter().any(|row| row.contains("Emacs Tutorial"))
        && grid.iter().any(|row| row.contains("About GNU"))
        && grid.iter().any(|row| row.contains("Help >"))
}

fn file_menu_ready(grid: &[String]) -> bool {
    grid.iter().any(|row| row.contains("Visit New File"))
        && grid.iter().any(|row| row.contains("Open File"))
}

/// The menu-bar row and column where GNU marks the active heading.
fn heading_marker(grid: &[String]) -> Option<(usize, usize)> {
    grid.iter()
        .enumerate()
        .find_map(|(row, line)| line.find("Help >").map(|col| (row, col)))
}

/// The widest run of dashes above the mode line.  The mode line itself is
/// mostly dashes, so the search must stay inside the menu's rows: GNU draws
/// a menu separator across the whole pane, which is the pane width.
fn widest_dash_run(grid: &[String], mode_line_row: usize) -> Option<(usize, usize, usize)> {
    grid.iter()
        .take(mode_line_row)
        .enumerate()
        .filter_map(|(row, line)| {
            let trimmed = line.trim_end();
            let start = trimmed.find('-')?;
            let run = trimmed[start..].chars().take_while(|&ch| ch == '-').count();
            (run >= 10).then_some((row, start, run))
        })
        .max_by_key(|&(_, _, run)| run)
}

#[test]
fn tty_help_menu_dropdown_grid_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "F10");
    gnu.read_until(Duration::from_secs(8), file_menu_ready);
    neo.read_until(Duration::from_secs(12), file_menu_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "RIGHT RIGHT RIGHT RIGHT RIGHT RIGHT");
    gnu.read_until(Duration::from_secs(8), help_menu_ready);
    neo.read_until(Duration::from_secs(12), help_menu_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let gnu_grid = gnu.text_grid();
    let neo_grid = neo.text_grid();
    if !help_menu_ready(&gnu_grid) || !help_menu_ready(&neo_grid) {
        dump_pair_grids("tty_help_menu_dropdown_grid_matches_gnu/open", &gnu, &neo);
    }
    assert!(
        help_menu_ready(&gnu_grid),
        "GNU should show the Help drop-down with its title marker"
    );
    assert!(
        help_menu_ready(&neo_grid),
        "Neomacs should show the Help drop-down with its title marker"
    );

    // The active heading is marked "Help >" on the menu-bar row, at the same
    // column as GNU's menu-bar item.
    let gnu_heading = heading_marker(&gnu_grid);
    let neo_heading = heading_marker(&neo_grid);
    assert_eq!(
        neo_heading, gnu_heading,
        "Neomacs must mark the active heading where GNU does"
    );
    let (heading_row, heading_col) = gnu_heading.expect("GNU heading marker");
    assert!(
        gnu_grid[heading_row].contains("File Edit"),
        "the heading marker belongs on the menu-bar row"
    );
    assert!(
        neo_grid[heading_row].contains("File Edit"),
        "Neomacs must mark the heading on the menu-bar row"
    );
    assert!(
        neo_grid[heading_row + 1].contains("Emacs Tutorial"),
        "the first item must start on the row below the marked heading"
    );

    // GNU draws separators as a run of dashes spanning the pane width, on the
    // item column and padded by one blank on each side.
    let mode_line_row = gnu_grid
        .iter()
        .position(|row| row.contains("F1  *scratch*"))
        .unwrap_or(gnu_grid.len());
    let gnu_separator = widest_dash_run(&gnu_grid, mode_line_row).expect("GNU menu separator");
    let neo_separator = widest_dash_run(&neo_grid, mode_line_row);
    assert_eq!(
        neo_separator,
        Some(gnu_separator),
        "Neomacs must draw the separator at GNU's row and column, at GNU's width"
    );
    let (separator_row, separator_col, separator_len) = gnu_separator;
    assert!(
        separator_row > heading_row,
        "the separator is a menu item below the heading"
    );
    assert_eq!(
        separator_col, heading_col,
        "items and separators share the pane's left column"
    );
    let separator_region =
        &gnu_grid[separator_row][separator_col - 1..separator_col + separator_len + 1];
    assert_eq!(
        separator_region,
        format!(" {} ", "-".repeat(separator_len)),
        "GNU pads a dash separator with one blank on each side"
    );
    assert_eq!(
        &neo_grid[separator_row][separator_col - 1..separator_col + separator_len + 1],
        separator_region,
        "Neomacs must pad its dash separator the same way"
    );

    // Every row the menu covers must match GNU's text exactly, which pins
    // the item padding, the two-space key-equivalent column, and the
    // submenu " >" markers too.
    for row in heading_row..mode_line_row {
        assert_eq!(
            neo_grid[row].trim_end(),
            gnu_grid[row].trim_end(),
            "menu row {row} must match GNU"
        );
    }

    send_both(&mut gnu, &mut neo, "C-g");
    let closed = |grid: &[String]| scratch_ready(grid) && !help_menu_ready(grid);
    gnu.read_until(Duration::from_secs(8), closed);
    neo.read_until(Duration::from_secs(12), closed);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert!(
        closed(&gnu.text_grid()),
        "GNU should close the Help menu on C-g"
    );
    assert!(
        closed(&neo.text_grid()),
        "Neomacs should close the Help menu on C-g"
    );
    assert_pair_exact_display("tty_help_menu_dropdown_grid_matches_gnu/close", &gnu, &neo);
}
