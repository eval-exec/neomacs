#![cfg(unix)]
//! TUI comparisons for common Org mode workflows.
//!
//! GNU behavior here is driven by `lisp/org/org.el`,
//! `lisp/org/org-cycle.el`, `lisp/org/org-keys.el`, and
//! `lisp/org/org-list.el`.  Table behavior is driven by
//! `lisp/org/org-table.el`; `C-c C-c` reaches it through
//! `org-ctrl-c-ctrl-c`.

mod support;

use std::time::Duration;
use support::*;

fn grid_contains(session: &neomacs_tui_tests::TuiSession, needle: &str) -> bool {
    session.text_grid().iter().any(|row| row.contains(needle))
}

fn assert_org_block_blank_line_extends_block_background(session: &neomacs_tui_tests::TuiSession) {
    let grid = session.text_grid();
    let code_row = grid
        .iter()
        .position(|row| row.contains("(message \"block\")"))
        .unwrap_or_else(|| {
            panic!(
                "{} should display the org source body line\n{}",
                session.name,
                grid.join("\n")
            )
        });
    let blank_row = code_row + 1;
    let code_col = grid[code_row]
        .find("(message \"block\")")
        .expect("source body column");
    let Some(code_cell) = session.screen().cell(code_row as u16, code_col as u16) else {
        panic!("{} missing code cell", session.name);
    };
    let Some(blank_cell) = session.screen().cell(blank_row as u16, code_col as u16) else {
        panic!("{} missing blank block cell", session.name);
    };

    assert!(
        blank_cell.contents().trim().is_empty(),
        "{} expected an empty source-block row below the code line, got {:?}\n{}",
        session.name,
        blank_cell.contents(),
        grid.join("\n")
    );
    assert_eq!(
        blank_cell.bgcolor(),
        code_cell.bgcolor(),
        "{} should extend org-block background across blank source-block rows at row {blank_row}, col {code_col}; code bg {:?}, blank bg {:?}\n{}",
        session.name,
        code_cell.bgcolor(),
        blank_cell.bgcolor(),
        grid.join("\n")
    );
}

#[test]
fn org_block_background_extends_over_blank_source_rows() {
    let (mut gnu, mut neo) = boot_pair("");

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(custom-set-faces '(org-block ((t (:background "gray93")))))"#,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("nil"))
    });

    open_home_file(
        &mut gnu,
        &mut neo,
        "org-block-bg-probe.org",
        r#"#+begin_src emacs-lisp
(message "block")

#+end_src
"#,
        "C-x C-f",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("(message \"block\")"))
    });
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_org_block_blank_line_extends_block_background(&gnu);
    assert_org_block_blank_line_extends_block_background(&neo);
    assert_pair_exact_display(
        "org_block_background_extends_over_blank_source_rows",
        &gnu,
        &neo,
    );
}

#[test]
fn org_todo_via_cc_ct_cycles_heading_keyword() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "org-todo-probe.org";
    let initial = "* Task\n";
    let expected = "* DONE Task\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-c C-t");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("* TODO Task"))
    });
    send_both(&mut gnu, &mut neo, "C-c C-t");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("* DONE Task"))
    });

    save_current_file_and_assert_contents("org-todo", &mut gnu, &mut neo, name, expected);
    assert_pair_exact_display("org_todo_via_cc_ct_cycles_heading_keyword", &gnu, &neo);
}

#[test]
fn org_meta_return_inserts_same_level_heading() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "org-meta-return-probe.org";
    let initial = "* First\n* Second\n";
    let expected = "* First\n* Inserted\n* Second\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-e ESC RET");
    gnu.send(b"Inserted");
    neo.send(b"Inserted");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("* Inserted"))
    });

    save_current_file_and_assert_contents("org-meta-return", &mut gnu, &mut neo, name, expected);
    assert_pair_exact_display("org_meta_return_inserts_same_level_heading", &gnu, &neo);
}

#[test]
fn org_tab_local_cycle_folds_and_reveals_subtree() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "org-cycle-probe.org";
    let initial = "* Parent\nbody line\n** Child\nchild body\n* Next\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "TAB");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for session in [&gnu, &neo] {
        assert!(
            grid_contains(session, "* Parent"),
            "{} should keep the folded parent heading visible",
            session.name
        );
        assert!(
            !grid_contains(session, "body line") && !grid_contains(session, "** Child"),
            "{} should hide subtree body and children after first TAB",
            session.name
        );
    }

    send_both(&mut gnu, &mut neo, "TAB TAB");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("body line"))
            && grid.iter().any(|row| row.contains("** Child"))
            && grid.iter().any(|row| row.contains("child body"))
    });
    assert_pair_exact_display("org_tab_local_cycle_folds_and_reveals_subtree", &gnu, &neo);
}

#[test]
fn org_table_ctrl_c_ctrl_c_aligns_columns() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "org-table-align-probe.org";
    let initial = "| Name | Qty |\n| apple | 2 |\n| banana | 10 |\n";
    let expected = "| Name   | Qty |\n| apple  |   2 |\n| banana |  10 |\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-f C-f C-c C-c");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("| Name   | Qty |"))
            && grid.iter().any(|row| row.contains("| banana |  10 |"))
    });

    if !grid_contains(&gnu, "| banana |  10 |") || !grid_contains(&neo, "| banana |  10 |") {
        dump_pair_grids("org-table-align", &gnu, &neo);
    }

    save_current_file_and_assert_contents("org-table-align", &mut gnu, &mut neo, name, expected);
    assert_pair_exact_display("org_table_ctrl_c_ctrl_c_aligns_columns", &gnu, &neo);
}

#[test]
fn org_table_tblfm_ctrl_c_ctrl_c_recalculates_sum() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "org-table-formula-probe.org";
    let initial = "| item | value |\n\
|------+-------|\n\
| a | 2 |\n\
| b | 3 |\n\
|------+-------|\n\
| total |  |\n\
#+TBLFM: @>$2=vsum(@2..@-1)\n";
    let expected = "| item  | value |\n\
|-------+-------|\n\
| a     |     2 |\n\
| b     |     3 |\n\
|-------+-------|\n\
| total |     5 |\n\
#+TBLFM: @>$2=vsum(@2..@-1)\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (goto-char (point-min)) (search-forward \"TBLFM\") nil)",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("nil"))
    });

    send_both(&mut gnu, &mut neo, "C-c C-c");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("| total |     5 |"))
    });

    if !grid_contains(&gnu, "| total |     5 |") || !grid_contains(&neo, "| total |     5 |") {
        dump_pair_grids("org-table-tblfm", &gnu, &neo);
    }

    save_current_file_and_assert_contents("org-table-tblfm", &mut gnu, &mut neo, name, expected);
    assert_pair_exact_display("org_table_tblfm_ctrl_c_ctrl_c_recalculates_sum", &gnu, &neo);
}

#[test]
fn org_babel_python_source_block_inserts_output_results() {
    assert!(
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok(),
        "org-babel python TUI test requires python3 in PATH"
    );

    let (mut gnu, mut neo) = boot_pair("");
    let name = "org-babel-python-hello.org";
    let initial =
        "#+begin_src python :results output :python python3\nprint(\"hello world\")\n#+end_src\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (require 'ob-python) (setq-local org-confirm-babel-evaluate nil))",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("nil"))
    });

    send_both(&mut gnu, &mut neo, "C-c C-c");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(20), |grid| {
        grid.iter().any(|row| row.contains("#+RESULTS:"))
            && grid.iter().any(|row| row.contains("hello world"))
    });

    if !grid_contains(&gnu, "#+RESULTS:") || !grid_contains(&neo, "#+RESULTS:") {
        dump_pair_grids("org-babel-python-hello", &gnu, &neo);
    }

    for session in [&gnu, &neo] {
        assert!(
            grid_contains(session, "#+RESULTS:"),
            "{} should insert an Org Babel results drawer",
            session.name
        );
        assert!(
            grid_contains(session, "hello world"),
            "{} should display python stdout in the results",
            session.name
        );
    }

    save_current_file_and_assert_contents(
        "org-babel-python-hello",
        &mut gnu,
        &mut neo,
        name,
        "#+begin_src python :results output :python python3\n\
print(\"hello world\")\n\
#+end_src\n\
\n\
#+RESULTS:\n\
: hello world\n",
    );
    assert_pair_exact_display(
        "org_babel_python_source_block_inserts_output_results",
        &gnu,
        &neo,
    );
}
