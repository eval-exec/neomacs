//! TUI comparison tests: windows tabs.

mod support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Local helpers ───────────────────────────────────────────

fn grid_has_two_scratch_windows(grid: &[String]) -> bool {
    grid.iter().filter(|row| row.contains("*scratch*")).count() >= 2
}

fn wait_for_split_window_below(gnu: &mut TuiSession, neo: &mut TuiSession) {
    let timeout = Duration::from_secs(5);
    gnu.read_until(timeout, grid_has_two_scratch_windows);
    neo.read_until(timeout, grid_has_two_scratch_windows);
}

fn wait_for_other_window_after_split(gnu: &mut TuiSession, neo: &mut TuiSession) {
    let timeout = Duration::from_secs(5);
    gnu.read_until(timeout, |grid| {
        grid_has_two_scratch_windows(grid)
            && grid
                .last()
                .is_some_and(|row| !row.contains("No other window to select"))
    });
    neo.read_until(timeout, |grid| {
        grid_has_two_scratch_windows(grid)
            && grid
                .last()
                .is_some_and(|row| !row.contains("No other window to select"))
    });
}

fn screen_cells_for_text(
    screen: &vt100::Screen,
    needle: &str,
) -> Vec<(u16, u16, vt100::Color, vt100::Color)> {
    let (rows, cols) = screen.size();
    let mut cells = Vec::new();
    for row in 0..rows.saturating_sub(1) {
        let text = screen.contents_between(row, 0, row, cols);
        let mut start = 0usize;
        while let Some(offset) = text[start..].find(needle) {
            let col = start + offset;
            if let Some(cell) = screen.cell(row, col as u16) {
                cells.push((row, col as u16, cell.fgcolor(), cell.bgcolor()));
            }
            start = col + needle.len();
        }
    }
    cells
}

fn assert_split_buffer_text_has_same_face(session: &TuiSession, needle: &str) {
    let cells = screen_cells_for_text(session.screen(), needle);
    assert!(
        cells.len() >= 2,
        "{} should show {needle:?} in both split windows; found {cells:?}\n{}",
        session.name,
        session.text_grid().join("\n")
    );

    let first = cells[0];
    let second = cells[1];
    assert_eq!(
        (second.2, second.3),
        (first.2, first.3),
        "{} should render {needle:?} with the same face in both split windows; cells={cells:?}\n{}",
        session.name,
        session.text_grid().join("\n")
    );
}

fn assert_split_blank_body_backgrounds_match(session: &TuiSession) {
    let (rows, cols) = session.screen().size();
    let left_col = 4;
    let right_col = cols / 2 + 4;
    let mut checked = 0usize;
    let mut mismatches = Vec::new();

    for row in 4..rows.saturating_sub(2) {
        let (Some(left), Some(right)) = (
            session.screen().cell(row, left_col),
            session.screen().cell(row, right_col),
        ) else {
            continue;
        };
        if !left.contents().trim().is_empty() || !right.contents().trim().is_empty() {
            continue;
        }
        checked += 1;
        if left.bgcolor() != right.bgcolor() && mismatches.len() < 8 {
            mismatches.push(format!(
                "row {row}: left col {left_col} bg {:?}, right col {right_col} bg {:?}",
                left.bgcolor(),
                right.bgcolor()
            ));
        }
    }

    assert!(
        checked > 10,
        "{} should have blank cells in both split windows; checked {checked}\n{}",
        session.name,
        session.text_grid().join("\n")
    );
    assert!(
        mismatches.is_empty(),
        "{} split blank body backgrounds differ:\n{}\n{}",
        session.name,
        mismatches.join("\n"),
        session.text_grid().join("\n")
    );
}

// ── Tests ──────────────────────────────────────────────────
#[test]
fn kill_buffer_and_window_via_cx4_0_restores_single_window() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-buffer-window.txt",
        "temporary other-window file\n",
        "C-x 4 C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x 4 0");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && !grid
                .iter()
                .any(|row| row.contains("kill-buffer-window.txt"))
            && !grid
                .iter()
                .any(|row| row.contains("temporary other-window file"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "kill_buffer_and_window_via_cx4_0_restores_single_window",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn tab_bar_new_next_and_close_via_cx_t_prefix() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "tab-one.txt",
        "tab one body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x t 2");
    let new_tab_ready = |grid: &[String]| grid.iter().any(|row| row.contains("*scratch*"));
    gnu.read_until(Duration::from_secs(6), new_tab_ready);
    neo.read_until(Duration::from_secs(8), new_tab_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    open_home_file(
        &mut gnu,
        &mut neo,
        "tab-two.txt",
        "tab two body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x t o");
    let first_tab_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("tab-one.txt"))
            && grid.iter().any(|row| row.contains("tab one body"))
    };
    gnu.read_until(Duration::from_secs(6), first_tab_ready);
    neo.read_until(Duration::from_secs(8), first_tab_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x t o");
    let second_tab_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("tab-two.txt"))
            && grid.iter().any(|row| row.contains("tab two body"))
    };
    gnu.read_until(Duration::from_secs(6), second_tab_ready);
    neo.read_until(Duration::from_secs(8), second_tab_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x t 0");
    gnu.read_until(Duration::from_secs(6), first_tab_ready);
    neo.read_until(Duration::from_secs(8), first_tab_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("tab_bar_new_next_and_close_via_cx_t_prefix", &gnu, &neo, 2);
}

#[test]
fn default_face_remap_renders_same_buffer_consistently_after_cx3_split() {
    let (mut gnu, mut neo) = boot_pair("");

    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn (require 'face-remap) (erase-buffer) (insert "REMAPPED-SPLIT\nsecond line\n") (goto-char (point-min)) (face-remap-add-relative 'default '(:foreground "#ffffff" :background "#000000")) (redisplay t))"##,
    );
    let remapped_text_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("REMAPPED-SPLIT"));
    wait_for_both(
        &mut gnu,
        &mut neo,
        Duration::from_secs(8),
        remapped_text_ready,
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x 3");
    let split_ready = |grid: &[String]| {
        grid.iter()
            .filter(|row| row.matches("REMAPPED-SPLIT").count() >= 2)
            .count()
            >= 1
            && grid_has_two_scratch_windows(grid)
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), split_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_split_buffer_text_has_same_face(&gnu, "REMAPPED-SPLIT");
    assert_split_buffer_text_has_same_face(&neo, "REMAPPED-SPLIT");
}

#[test]
fn scratch_comment_face_renders_same_buffer_consistently_after_cx3_split() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x 3");
    let split_ready = |grid: &[String]| {
        grid.iter()
            .filter(|row| row.matches("This buffer is for text").count() >= 2)
            .count()
            >= 1
            && grid_has_two_scratch_windows(grid)
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), split_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_split_buffer_text_has_same_face(&gnu, "This buffer is for text");
    assert_split_buffer_text_has_same_face(&neo, "This buffer is for text");
    assert_split_blank_body_backgrounds_match(&gnu);
    assert_split_blank_body_backgrounds_match(&neo);
}

#[test]
fn split_window_then_open_file_in_other_window_via_cx2_cxo_cx_cf() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "split-window.txt", "split line 1\nsplit line 2\n");
    write_home_file(&neo, "split-window.txt", "split line 1\nsplit line 2\n");

    send_both(&mut gnu, &mut neo, "C-x 2");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let minibuffer_path = "~/split-window.txt";
    gnu.send(minibuffer_path.as_bytes());
    neo.send(minibuffer_path.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("split line 1"))
            && grid.iter().any(|row| row.contains("split-window.txt"))
            && grid.iter().any(|row| row.contains("*scratch*"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "split_window_then_open_file_in_other_window_via_cx2_cxo_cx_cf",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn split_window_right_then_open_file_in_other_window_via_cx3_cxo_cx_cf() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(
        &gnu,
        "split-window-right.txt",
        "right split line 1\nright split line 2\n",
    );
    write_home_file(
        &neo,
        "split-window-right.txt",
        "right split line 1\nright split line 2\n",
    );

    send_both(&mut gnu, &mut neo, "C-x 3");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let minibuffer_path = "~/split-window-right.txt";
    gnu.send(minibuffer_path.as_bytes());
    neo.send(minibuffer_path.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("right split line 1"))
            && grid
                .iter()
                .any(|row| row.contains("split-window-right.txt"))
            && grid.iter().any(|row| row.contains("*scratch*"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "split_window_right_then_open_file_in_other_window_via_cx3_cxo_cx_cf",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn compare_windows_via_mx_advances_both_points_to_first_difference() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "compare-left.txt", "same prefix\nleft side differs\n");
    write_home_file(&neo, "compare-left.txt", "same prefix\nleft side differs\n");
    write_home_file(
        &gnu,
        "compare-right.txt",
        "same prefix\nright side differs\n",
    );
    write_home_file(
        &neo,
        "compare-right.txt",
        "same prefix\nright side differs\n",
    );

    open_home_file(
        &mut gnu,
        &mut neo,
        "compare-left.txt",
        "same prefix\nleft side differs\n",
        "C-x C-f",
    );
    send_both(&mut gnu, &mut neo, "C-x 3 C-x o C-x C-f");
    for session in [&mut gnu, &mut neo] {
        session.send(b"~/compare-right.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let both_files_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("left side differs"))
            && grid.iter().any(|row| row.contains("right side differs"))
    };
    gnu.read_until(Duration::from_secs(6), both_files_ready);
    neo.read_until(Duration::from_secs(8), both_files_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    invoke_mx_command(&mut gnu, &mut neo, "compare-windows");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(br#"(message "compare-points %S/%S" (point) (window-point (next-window)))"#);
    }
    send_both(&mut gnu, &mut neo, "RET");

    let point_ready = |grid: &[String]| grid.iter().any(|row| row.contains("compare-points 13/13"));
    gnu.read_until(Duration::from_secs(6), point_ready);
    neo.read_until(Duration::from_secs(8), point_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            point_ready(&grid),
            "{label} should leave both windows at first difference:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_nearly_matches(
        "compare_windows_via_mx_advances_both_points_to_first_difference",
        &gnu,
        &neo,
        3,
    );
}

#[test]
fn other_window_via_cxo() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "other-window-hop.txt",
        "window body\n",
        "C-x 2 C-x o C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-a");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"BOTTOM ");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"TOP ");
    }

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("TOP ;; This buffer is for text that is not saved"))
            && grid.iter().any(|row| row.contains("BOTTOM window body"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("other_window_via_cxo", &gnu, &neo, 2);
}

#[test]
fn other_window_numeric_prefix_skips_windows_in_cycle_order() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "window-cycle-a.txt",
        "window cycle A\n",
        "C-x C-f",
    );
    send_both(&mut gnu, &mut neo, "C-x 2 C-x o C-x C-f");
    for session in [&mut gnu, &mut neo] {
        session.send(b"~/window-cycle-b.txt");
    }
    write_home_file(&gnu, "window-cycle-b.txt", "window cycle B\n");
    write_home_file(&neo, "window-cycle-b.txt", "window cycle B\n");
    send_both(&mut gnu, &mut neo, "RET");

    let two_windows_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("window cycle A"))
            && grid.iter().any(|row| row.contains("window cycle B"))
    };
    gnu.read_until(Duration::from_secs(6), two_windows_ready);
    neo.read_until(Duration::from_secs(8), two_windows_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x 3 C-x o C-x C-f");
    write_home_file(&gnu, "window-cycle-c.txt", "window cycle C\n");
    write_home_file(&neo, "window-cycle-c.txt", "window cycle C\n");
    for session in [&mut gnu, &mut neo] {
        session.send(b"~/window-cycle-c.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let three_windows_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("window cycle A"))
            && grid.iter().any(|row| row.contains("window cycle B"))
            && grid.iter().any(|row| row.contains("window cycle C"))
    };
    gnu.read_until(Duration::from_secs(6), three_windows_ready);
    neo.read_until(Duration::from_secs(8), three_windows_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-u 2 C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    for session in [&mut gnu, &mut neo] {
        session.send(b"SELECTED ");
    }

    let selected_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("SELECTED window cycle B"))
            && grid.iter().any(|row| row.contains("window cycle A"))
            && grid.iter().any(|row| row.contains("window cycle C"))
            && !grid
                .iter()
                .any(|row| row.contains("SELECTED window cycle A"))
            && !grid
                .iter()
                .any(|row| row.contains("SELECTED window cycle C"))
    };
    gnu.read_until(Duration::from_secs(6), selected_ready);
    neo.read_until(Duration::from_secs(8), selected_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            selected_ready(&grid),
            "{label} should select the GNU cycle-order target for C-u 2 C-x o\n{}",
            grid.join("\n")
        );
    }
    assert_pair_nearly_matches(
        "other_window_numeric_prefix_skips_windows_in_cycle_order",
        &gnu,
        &neo,
        3,
    );
}

#[test]
fn delete_other_windows_after_find_file_other_window_via_cx1() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "single-window.txt",
        "window collapse\n",
        "C-x 4 C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x 1");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("single-window.txt"))
            && grid.iter().any(|row| row.contains("window collapse"))
            && grid.iter().filter(|row| row.contains("*scratch*")).count() == 0
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "delete_other_windows_after_find_file_other_window_via_cx1",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn delete_selected_other_window_via_cx0() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-window.txt",
        "delete me window\n",
        "C-x 2 C-x o C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x 0");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
            && !grid.iter().any(|row| row.contains("delete-window.txt"))
            && !grid.iter().any(|row| row.contains("delete me window"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("delete_selected_other_window_via_cx0", &gnu, &neo, 2);
}

#[test]
fn enlarge_then_balance_windows_via_cx_caret_and_plus() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "resize-windows.txt",
        "top window\nbottom window\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x 2");
    let split_ready = |grid: &[String]| {
        grid.iter()
            .filter(|row| row.contains("resize-windows.txt"))
            .count()
            >= 2
    };
    gnu.read_until(Duration::from_secs(6), split_ready);
    neo.read_until(Duration::from_secs(8), split_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x ^");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(
            br#"(message "resize-window-taller %S" (> (window-total-height) (window-total-height (next-window))))"#,
        );
    }
    send_both(&mut gnu, &mut neo, "RET");

    let taller_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("resize-window-taller t"))
    };
    gnu.read_until(Duration::from_secs(6), taller_ready);
    neo.read_until(Duration::from_secs(8), taller_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            taller_ready(&grid),
            "{label} should make selected window taller after C-x ^:\n{}",
            grid.join("\n")
        );
    }

    send_both(&mut gnu, &mut neo, "C-x +");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "M-:");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(
            br#"(message "resize-window-balanced %S" (<= (abs (- (window-total-height) (window-total-height (next-window)))) 1))"#,
        );
    }
    send_both(&mut gnu, &mut neo, "RET");

    let balanced_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("resize-window-balanced t"))
    };
    gnu.read_until(Duration::from_secs(6), balanced_ready);
    neo.read_until(Duration::from_secs(8), balanced_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            balanced_ready(&grid),
            "{label} should balance split window heights after C-x +:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_nearly_matches(
        "enlarge_then_balance_windows_via_cx_caret_and_plus",
        &gnu,
        &neo,
        3,
    );
}

#[test]
fn window_configuration_to_register_and_jump_via_cx_r_w_j() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "window-register.txt",
        "alpha window register\nbeta window register\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x 2");
    let split_ready = |grid: &[String]| {
        grid.iter()
            .filter(|row| row.contains("window-register.txt"))
            .count()
            >= 2
    };
    gnu.read_until(Duration::from_secs(6), split_ready);
    neo.read_until(Duration::from_secs(8), split_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x r w");
    let window_register_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Window configuration to register:"))
    };
    gnu.read_until(Duration::from_secs(6), window_register_prompt);
    neo.read_until(Duration::from_secs(8), window_register_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"a");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x 1");
    let single_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("window-register.txt"))
            && grid
                .iter()
                .filter(|row| row.contains("window-register.txt"))
                .count()
                == 1
    };
    gnu.read_until(Duration::from_secs(6), single_ready);
    neo.read_until(Duration::from_secs(8), single_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x r j");
    let jump_register_prompt =
        |grid: &[String]| grid.iter().any(|row| row.contains("Jump to register:"));
    gnu.read_until(Duration::from_secs(6), jump_register_prompt);
    neo.read_until(Duration::from_secs(8), jump_register_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"a");
    }

    gnu.read_until(Duration::from_secs(6), split_ready);
    neo.read_until(Duration::from_secs(8), split_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "window_configuration_to_register_and_jump_via_cx_r_w_j",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn scroll_other_window_via_cmv() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=80 {
        contents.push_str(&format!("other scroll {line:02}\n"));
    }
    write_home_file(&gnu, "other-scroll.txt", &contents);
    write_home_file(&neo, "other-scroll.txt", &contents);

    send_both(&mut gnu, &mut neo, "C-x 2");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x C-f");
    let minibuffer_path = "~/other-scroll.txt";
    gnu.send(minibuffer_path.as_bytes());
    neo.send(minibuffer_path.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");

    let opened = |grid: &[String]| {
        grid.iter().any(|row| row.contains("other scroll 01"))
            && grid.iter().any(|row| row.contains("*scratch*"))
    };
    gnu.read_until(Duration::from_secs(6), opened);
    neo.read_until(Duration::from_secs(8), opened);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-M-v");

    let scrolled = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid.iter().any(|row| row.contains("other scroll 20"))
    };
    gnu.read_until(Duration::from_secs(6), scrolled);
    neo.read_until(Duration::from_secs(8), scrolled);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("scroll_other_window_via_cmv", &gnu, &neo, 2);
}

#[test]
fn split_window_below() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-x 2");
    wait_for_split_window_below(&mut gnu, &mut neo);

    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    let diffs = meaningful_diffs(diff_text_grids(&gl, &nl));
    if !diffs.is_empty() {
        eprintln!("split_window_below: {} rows differ", diffs.len());
        print_row_diffs(&diffs);
    }
    assert!(
        diffs.len() <= 2,
        "C-x 2 screens differ in {} rows",
        diffs.len()
    );
}

#[test]
fn split_window_right() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-x 3");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    let diffs = meaningful_diffs(diff_text_grids(&gl, &nl));
    if !diffs.is_empty() {
        eprintln!("split_window_right: {} rows differ", diffs.len());
        print_row_diffs(&diffs);
    }
    assert!(
        diffs.len() <= 2,
        "C-x 3 screens differ in {} rows",
        diffs.len()
    );
}

#[test]
fn other_window_after_split() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-x 2");
    wait_for_split_window_below(&mut gnu, &mut neo);
    send_both(&mut gnu, &mut neo, "C-x o");
    wait_for_other_window_after_split(&mut gnu, &mut neo);

    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    let diffs = meaningful_diffs(diff_text_grids(&gl, &nl));
    if !diffs.is_empty() {
        eprintln!("other_window_after_split: {} rows differ", diffs.len());
        print_row_diffs(&diffs);
    }
    // Allow some tolerance for cursor position display
    assert!(
        diffs.len() <= 3,
        "C-x 2, C-x o screens differ in {} rows",
        diffs.len()
    );
}

// ── Frame management tests ──────────────────────────────────

#[test]
fn make_frame_and_delete_frame_via_cx5() {
    let (mut gnu, mut neo) = boot_pair("");

    // C-x 5 2 creates a new frame
    send_both(&mut gnu, &mut neo, "C-x 5 2");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Both should show a frame — on TTY this creates a new "screen"
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            !grid.iter().all(|row| row.trim().is_empty()),
            "{label}: screen should not be blank after C-x 5 2"
        );
    }

    // Delete the new frame with C-x 5 0
    send_both(&mut gnu, &mut neo, "C-x 5 0");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Should return to a valid screen (not crash or hang)
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            !grid.iter().all(|row| row.trim().is_empty()),
            "{label}: screen should not be blank after C-x 5 0"
        );
    }

    assert_pair_nearly_matches("make_frame_and_delete_frame_via_cx5", &gnu, &neo, 2);
}

#[test]
fn window_point_independence_after_split_and_cursor_moves() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "win-pt.txt",
        "line one\nline two\nline three\nline four\n",
        "C-x C-f",
    );

    // Split window below, switch to bottom
    send_both(&mut gnu, &mut neo, "C-x 2 C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Move down in bottom window
    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    // Switch back to top
    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("line one")),
            "{label}: should show buffer content after window split ops"
        );
    }

    assert_pair_nearly_matches(
        "window_point_independence_after_split_and_cursor_moves",
        &gnu,
        &neo,
        3,
    );
}

#[test]
fn split_window_and_switch_via_cx_o_shows_other_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    // Open a file so we have something to see in both windows
    open_home_file(
        &mut gnu,
        &mut neo,
        "win-split.txt",
        "split test content\n",
        "C-x C-f",
    );

    // Split window: C-x 2
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "2");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Switch to other window: C-x o
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("split test content")),
            "{label}: after C-x 2 C-x o, should show file content"
        );
    }
}

#[test]
fn delete_other_windows_via_cx_1_leaves_single_window() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "del-other.txt",
        "only window\n",
        "C-x C-f",
    );
    // Split then delete other
    send_both(&mut gnu, &mut neo, "C-x 2");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both(&mut gnu, &mut neo, "C-x 1");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("only window")),
            "{label}: C-x 2 C-x 1 should leave single window with content"
        );
    }
}

// ── Per-window tab-line (global-tab-line-mode) ──────────────────────────
//
// Regression for the bug where every window's tab line showed the buffer of
// the globally selected window instead of its own buffer.  Root cause: the
// redisplay path did not rebind `selected-window` to the window whose
// mode/tab/header line was being evaluated, so the default
// `tab-line-tabs-function` (`tab-line-tabs-window-buffers`, which reads
// `(selected-window)`) saw the global selection.  GNU's `display_mode_lines`
// (src/xdisp.c) sets `selected_window = w` for exactly this reason.

/// Indices of rows whose tab line shows `name` as a tab.  Mode-line rows also
/// contain the buffer name but carry the major-mode marker `(Fundamental)`,
/// so they are excluded.
fn tab_line_rows(grid: &[String], name: &str) -> Vec<usize> {
    grid.iter()
        .enumerate()
        .filter(|(_, row)| row.contains(name) && !row.contains("Fundamental"))
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn global_tab_line_shows_each_window_its_own_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    // buffer1 above, buffer2 below; enable global-tab-line-mode *after* the
    // buffers are displayed so its globalized turn-on applies to them (a
    // globalized minor mode otherwise only turns on via the major-mode hook,
    // which `get-buffer-create` does not fire).  Clear each window's prev/next
    // buffers so its tab line shows exactly one tab (its own buffer).  Leaves
    // focus on the upper window (buffer1).
    let setup = "(progn \
        (with-current-buffer (get-buffer-create \"buffer1\") (erase-buffer) (insert \"ACONTENT\")) \
        (with-current-buffer (get-buffer-create \"buffer2\") (erase-buffer) (insert \"BCONTENT\")) \
        (delete-other-windows) \
        (switch-to-buffer \"buffer1\") \
        (split-window-below) \
        (other-window 1) \
        (switch-to-buffer \"buffer2\") \
        (other-window 1) \
        (global-tab-line-mode 1) \
        (dolist (w (window-list)) (set-window-prev-buffers w nil) (set-window-next-buffers w nil)) \
        (force-mode-line-update t))";
    eval_expression(&mut gnu, &mut neo, setup);

    let content_ready = |grid: &[String]| {
        grid.iter().any(|r| r.contains("ACONTENT")) && grid.iter().any(|r| r.contains("BCONTENT"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), content_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    fn assert_per_window_tab_lines(label: &str, gnu: &TuiSession, neo: &TuiSession) {
        let gg = gnu.text_grid();
        let ng = neo.text_grid();
        let g1 = tab_line_rows(&gg, "buffer1");
        let g2 = tab_line_rows(&gg, "buffer2");
        let n1 = tab_line_rows(&ng, "buffer1");
        let n2 = tab_line_rows(&ng, "buffer2");
        let ok = g1.len() == 1
            && g2.len() == 1
            && g1[0] < g2[0]
            && n1.len() == 1
            && n2.len() == 1
            && n1[0] < n2[0];
        if !ok {
            dump_pair_grids(label, gnu, neo);
            eprintln!(
                "{label}: GNU buffer1 tab rows={g1:?} buffer2 tab rows={g2:?}; \
                 NEO buffer1 tab rows={n1:?} buffer2 tab rows={n2:?}"
            );
        }
        // GNU reference behaviour: one tab line per buffer, buffer1 above buffer2.
        assert_eq!(g1.len(), 1, "{label}: GNU buffer1 tab-line count");
        assert_eq!(g2.len(), 1, "{label}: GNU buffer2 tab-line count");
        assert!(g1[0] < g2[0], "{label}: GNU buffer1 tab above buffer2");
        // neomacs must match GNU: each window's tab line shows its own buffer.
        assert_eq!(n1.len(), 1, "{label}: neomacs buffer1 tab-line count");
        assert_eq!(n2.len(), 1, "{label}: neomacs buffer2 tab-line count");
        assert!(n1[0] < n2[0], "{label}: neomacs buffer1 tab above buffer2");
    }

    // Focus on the upper window (buffer1): each window shows its own tab.
    assert_per_window_tab_lines("focus-upper", &gnu, &neo);

    // Move focus to the lower window (buffer2): tab lines must not change.
    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_per_window_tab_lines("focus-lower", &gnu, &neo);
}
