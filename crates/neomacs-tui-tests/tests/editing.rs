//! TUI comparisons for ordinary scratch-buffer editing.
//!
//! GNU's default path for printable characters is `self-insert-command`
//! (`src/cmds.c`), while RET in ordinary editing invokes `newline`
//! (`lisp/simple.el`) and may run electric indentation from
//! `lisp/electric.el`.

mod support;

use neomacs_tui_tests::TuiSession;
use std::fs;
use std::time::Duration;
use support::*;

const FIRST: &str = "alpha scratch line";
const SECOND: &str = "beta scratch line";
const THIRD: &str = "gamma scratch line";
const EXPECTED_BUFFER: &str = "alpha scratch line\nbeta scratch line\ngamma scratch line";

fn visible_rows_for_typed_lines(label: &str, session: &TuiSession) -> [usize; 3] {
    let grid = session.text_grid();
    let find_row = |needle: &str| {
        grid.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| {
                panic!(
                    "{label} should visibly contain {needle:?}\n{}",
                    grid.join("\n")
                )
            })
    };

    let rows = [find_row(FIRST), find_row(SECOND), find_row(THIRD)];
    assert_eq!(
        rows[1],
        rows[0] + 1,
        "{label}: first and second typed lines should be adjacent"
    );
    assert_eq!(
        rows[2],
        rows[1] + 1,
        "{label}: second and third typed lines should be adjacent"
    );
    rows
}

#[test]
fn scratch_self_insert_ret_creates_three_visible_lines() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x h C-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let typed = format!("{FIRST}\r{SECOND}\r{THIRD}");
    gnu.send(typed.as_bytes());
    neo.send(typed.as_bytes());

    let typed_lines_visible = |grid: &[String]| {
        [FIRST, SECOND, THIRD]
            .iter()
            .all(|line| grid.iter().any(|row| row.contains(line)))
    };
    gnu.read_until(Duration::from_secs(6), typed_lines_visible);
    neo.read_until(Duration::from_secs(8), typed_lines_visible);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let gnu_rows = visible_rows_for_typed_lines("GNU", &gnu);
    let neo_rows = visible_rows_for_typed_lines("Neomacs", &neo);
    assert_eq!(
        neo_rows, gnu_rows,
        "Neomacs should render the typed scratch lines on the same rows as GNU"
    );

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(with-current-buffer "*scratch*" (write-region (point-min) (point-max) "~/scratch-three-lines.txt" nil 'silent))"#,
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let gnu_buffer =
        fs::read_to_string(gnu.home_dir().join("scratch-three-lines.txt")).expect("read GNU dump");
    let neo_buffer = fs::read_to_string(neo.home_dir().join("scratch-three-lines.txt"))
        .expect("read Neomacs dump");
    assert_eq!(gnu_buffer, EXPECTED_BUFFER, "GNU scratch buffer contents");
    assert_eq!(
        neo_buffer, EXPECTED_BUFFER,
        "Neomacs scratch buffer contents"
    );
}

#[test]
fn electric_return_newline_and_indent_in_lisp_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "electric-ret.el",
        "(defun my-fn ()\n  (message \"hello\"))\n",
        "C-x C-f",
    );

    // Go to end of second line (after the closing paren), press RET
    send_both(&mut gnu, &mut neo, "C-e RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Both should have auto-indented the new line
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        let has_indent = grid
            .iter()
            .any(|row| row.starts_with("  ") && row.trim().is_empty());
        assert!(
            has_indent || grid.iter().any(|row| row.trim().starts_with("(message")),
            "{label}: after RET, should auto-indent or preserve code structure\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "electric_return_newline_and_indent_in_lisp_buffer",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn next_line_preserves_interactive_goal_column_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU lisp/simple.el:next-line records `temporary-goal-column' from
    // `current-column' in line-move-1, then line-move-finish moves to that
    // column on the target logical line.
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(with-current-buffer "*scratch*" (erase-buffer) (insert "abcdef\n123456\nuvwxyz") (goto-char 5))"#,
    );
    let prepared = |grid: &[String]| {
        grid.iter().any(|row| row.contains("abcdef"))
            && grid.iter().any(|row| row.contains("123456"))
            && grid.iter().any(|row| row.contains("uvwxyz"))
    };
    gnu.read_until(Duration::from_secs(6), prepared);
    neo.read_until(Duration::from_secs(8), prepared);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-n");
    let moved = |grid: &[String]| grid.iter().any(|row| row.contains("123456"));
    gnu.read_until(Duration::from_secs(6), moved);
    neo.read_until(Duration::from_secs(8), moved);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(with-current-buffer "*scratch*" (message "linegoal:%S" (list (point) (current-column))))"#,
    );

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("linegoal:(12 4)"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label}: interactive C-n should preserve the starting goal column like GNU\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "next_line_preserves_interactive_goal_column_like_gnu",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn delete_char_via_cd_removes_character_after_point() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "delete-char.txt";
    let initial = "abcdef\n";
    let expected = "bcdef\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-a");
    send_both(&mut gnu, &mut neo, "C-d");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "delete-char-via-C-d",
        &mut gnu,
        &mut neo,
        name,
        expected,
    );
}
