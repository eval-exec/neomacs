use super::super::scenario::PackageTuiPair;
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};
use std::fmt::Write as _;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn wait_for_both<F>(pair: &mut PackageTuiPair, timeout: Duration, predicate: F)
where
    F: Fn(&[String]) -> bool + Copy,
{
    pair.gnu.read_until(timeout, predicate);
    assert!(
        predicate(&pair.gnu.text_grid()),
        "GNU Helm Pydoc screen did not reach the expected state:\n{}",
        pair.gnu.text_grid().join("\n")
    );
    pair.neo.read_until(timeout, predicate);
    assert!(
        predicate(&pair.neo.text_grid()),
        "Neomacs Helm Pydoc screen did not reach the expected state:\n{}",
        pair.neo.text_grid().join("\n")
    );
}

pub(super) fn send_to_both<F>(pair: &mut PackageTuiPair, operation: F)
where
    F: Fn(&mut TuiSession),
{
    operation(&mut pair.gnu);
    operation(&mut pair.neo);
}

pub(super) fn open_pydoc(pair: &mut PackageTuiPair) {
    send_to_both(pair, |session| {
        session.send_key("M-x");
        session.send(b"helm-pydoc");
        session.send_key("RET");
    });
    wait_for_both(pair, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("Imported Modules"))
            && grid.iter().any(|row| row.contains("Installed Modules"))
    });
}

pub(super) fn filter_module(pair: &mut PackageTuiPair, module: &str) {
    send_to_both(pair, |session| session.send(module.as_bytes()));
    wait_for_both(pair, Duration::from_secs(12), |grid| {
        grid.iter()
            .any(|row| row.contains("pattern:") && row.contains(module))
            && grid.iter().any(|row| row.trim_start().starts_with(module))
    });
}

pub(super) fn open_and_filter_module(pair: &mut PackageTuiPair, module: &str) {
    open_pydoc(pair);
    filter_module(pair, module);
}

pub(super) fn open_action_menu(pair: &mut PackageTuiPair) {
    send_to_both(pair, |session| session.send_key("TAB"));
    wait_for_both(pair, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("Pydoc Module"))
            && grid.iter().any(|row| row.contains("View Source Code"))
            && grid
                .iter()
                .any(|row| row.contains("Import Module(from module import identifiers as name)"))
    });
}

pub(super) fn matching_rows(session: &TuiSession, needles: &[&str]) -> String {
    let mut output = String::new();
    for (row, contents) in session.text_grid().iter().enumerate() {
        if needles.iter().any(|needle| contents.contains(needle)) {
            let _ = writeln!(&mut output, "{row:02} |{}", contents.trim_end());
        }
    }
    output
}

pub(super) fn matching_indices(session: &TuiSession, needles: &[&str]) -> Vec<u16> {
    session
        .text_grid()
        .iter()
        .enumerate()
        .filter_map(|(row, contents)| {
            needles
                .iter()
                .any(|needle| contents.contains(needle))
                .then_some(row as u16)
        })
        .collect()
}

pub(super) fn exact_rows(session: &TuiSession, labels: &[&str]) -> String {
    let mut output = String::new();
    for (row, contents) in session.text_grid().iter().enumerate() {
        if labels.contains(&contents.trim()) {
            let _ = writeln!(&mut output, "{row:02} |{}", contents.trim_end());
        }
    }
    output
}

pub(super) fn assert_exact_rows_stage(
    pair: &PackageTuiPair,
    stage: &str,
    labels: &[&str],
    expected_rows: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let gnu_rows = exact_rows(&pair.gnu, labels);
    let neo_rows = exact_rows(&pair.neo, labels);
    expected_rows.assert_eq(&gnu_rows);
    if neo_rows != gnu_rows {
        divergences.push(format!(
            "{stage} exact rows differ:\nGNU:\n{gnu_rows}\nNeomacs:\n{neo_rows}"
        ));
    }
}

pub(super) fn assert_stage(
    pair: &PackageTuiPair,
    stage: &str,
    needles: &[&str],
    expected_rows: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let gnu_rows = matching_rows(&pair.gnu, needles);
    let neo_rows = matching_rows(&pair.neo, needles);
    expected_rows.assert_eq(&gnu_rows);
    if neo_rows != gnu_rows {
        divergences.push(format!(
            "{stage} semantic rows differ:\nGNU:\n{gnu_rows}\nNeomacs:\n{neo_rows}"
        ));
    }

    let gnu_indices = matching_indices(&pair.gnu, needles);
    let neo_indices = matching_indices(&pair.neo, needles);
    if neo_indices != gnu_indices {
        divergences.push(format!(
            "{stage} row indices differ: GNU {gnu_indices:?}, Neomacs {neo_indices:?}"
        ));
    }
    assert!(
        !gnu_indices.is_empty(),
        "at least one meaningful terminal row"
    );
    for row in gnu_indices {
        let gnu_snapshot = RawTerminalSnapshot::capture_rows(pair.gnu.screen(), row..row + 1);
        let neo_snapshot = RawTerminalSnapshot::capture_rows(pair.neo.screen(), row..row + 1);
        if gnu_snapshot != neo_snapshot {
            // plain_grid() hides faces; a raw-row mismatch is almost always
            // a face divergence, so spell the differing cells out.
            for (gnu_row, neo_row) in gnu_snapshot.rows.iter().zip(&neo_snapshot.rows) {
                for col in 0..25usize {
                    if let (Some(g), Some(n)) = (gnu_row.cells.get(col), neo_row.cells.get(col)) {
                        eprintln!(
                            "ROW{row}-COL{col}: GNU fg={:?} bg={:?} | NEO fg={:?} bg={:?}",
                            g.fgcolor(),
                            g.bgcolor(),
                            n.fgcolor(),
                            n.bgcolor()
                        );
                    }
                }
            }
            let mut style_diffs = String::new();
            for (gnu_row, neo_row) in gnu_snapshot.rows.iter().zip(&neo_snapshot.rows) {
                for (col, (gnu_cell, neo_cell)) in
                    gnu_row.cells.iter().zip(&neo_row.cells).enumerate()
                {
                    if gnu_cell != neo_cell {
                        style_diffs.push_str(&format!(
                            "col {col}: GNU fg={:?} bg={:?} bold={} underline={} inverse={} \
                             | NEO fg={:?} bg={:?} bold={} underline={} inverse={}\n",
                            gnu_cell.fgcolor(),
                            gnu_cell.bgcolor(),
                            gnu_cell.bold(),
                            gnu_cell.underline(),
                            gnu_cell.inverse(),
                            neo_cell.fgcolor(),
                            neo_cell.bgcolor(),
                            neo_cell.bold(),
                            neo_cell.underline(),
                            neo_cell.inverse(),
                        ));
                    }
                }
            }
            divergences.push(format!(
                "{stage} raw terminal row {row} differs:\n{style_diffs}GNU:\n{}Neomacs:\n{}",
                gnu_snapshot.plain_grid(),
                neo_snapshot.plain_grid()
            ));
        }
    }
}

/// Compare the two engines' live helm-buffer overlay-state dumps (written
/// by the prelude's repeating timer while the session is idle).
pub(super) fn assert_overlay_introspection(
    pair: &PackageTuiPair,
    stage: &str,
    divergences: &mut Vec<String>,
) {
    std::thread::sleep(Duration::from_secs(3));
    let read = |session: &TuiSession| {
        let path = session.home_dir().join("helm-overlay-state.txt");
        fs::read_to_string(&path).unwrap_or_else(|error| format!("unavailable: {error}"))
    };
    let gnu = read(&pair.gnu);
    let neo = read(&pair.neo);
    let read_window_dump = |session: &TuiSession| {
        let path = session.home_dir().join("helm-window-overlays.txt");
        fs::read_to_string(&path).unwrap_or_else(|error| format!("unavailable: {error}"))
    };
    eprintln!("WINDOW-OVERLAYS-GNU:\n{}", read_window_dump(&pair.gnu));
    eprintln!("WINDOW-OVERLAYS-NEO:\n{}", read_window_dump(&pair.neo));
    eprintln!("OVERLAY-STATE-GNU: {gnu}");
    eprintln!("OVERLAY-STATE-NEO: {neo}");
    if gnu != neo {
        divergences.push(format!(
            "{stage} overlay introspection differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"
        ));
    }
}

pub(super) fn assert_release_console(
    pair: &PackageTuiPair,
    stage: &str,
    expected: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let relative = "release workspace/release_console.py";
    let gnu =
        fs::read_to_string(pair.gnu.home_dir().join(relative)).expect("read GNU release console");
    let neo = fs::read_to_string(pair.neo.home_dir().join(relative))
        .expect("read Neomacs release console");
    expected.assert_eq(&gnu);
    if neo != gnu {
        divergences.push(format!(
            "{stage} saved source differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"
        ));
    }
}

pub(super) fn save_and_wait_for_release_console(
    pair: &mut PackageTuiPair,
    expected_fragment: &str,
) {
    send_to_both(pair, |session| session.send_keys("C-x C-s"));
    let relative = "release workspace/release_console.py";
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        pair.gnu.read(Duration::from_millis(10));
        pair.neo.read(Duration::from_millis(10));
        let gnu_saved = fs::read_to_string(pair.gnu.home_dir().join(relative))
            .is_ok_and(|contents| contents.contains(expected_fragment));
        let neo_saved = fs::read_to_string(pair.neo.home_dir().join(relative))
            .is_ok_and(|contents| contents.contains(expected_fragment));
        if gnu_saved && neo_saved {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "GNU and Neomacs did not save release_console.py containing {expected_fragment:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn capture_and_assert_pydoc_buffer(
    pair: &mut PackageTuiPair,
    stage: &str,
    file_stem: &str,
    expected_contents: expect_test::Expect,
    expected_state: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let expression = format!(
        r##"(with-current-buffer "*Pydoc deploymentkit*" (let ((state (list :point (point) :view-mode view-mode :read-only buffer-read-only :modified (buffer-modified-p)))) (write-region (point-min) (point-max) (expand-file-name "{file_stem}-buffer.txt" (getenv "HOME")) nil 'silent) (with-temp-file (expand-file-name "{file_stem}-state.txt" (getenv "HOME")) (insert (prin1-to-string state)))))"##
    );
    send_to_both(pair, |session| {
        session.send_key("M-:");
        session.send(expression.as_bytes());
        session.send_key("RET");
    });

    let deadline = Instant::now() + Duration::from_secs(8);
    let (gnu_contents, neo_contents, gnu_state, neo_state) = loop {
        pair.gnu.read(Duration::from_millis(10));
        pair.neo.read(Duration::from_millis(10));
        let files = (
            fs::read_to_string(pair.gnu.home_dir().join(format!("{file_stem}-buffer.txt"))),
            fs::read_to_string(pair.neo.home_dir().join(format!("{file_stem}-buffer.txt"))),
            fs::read_to_string(pair.gnu.home_dir().join(format!("{file_stem}-state.txt"))),
            fs::read_to_string(pair.neo.home_dir().join(format!("{file_stem}-state.txt"))),
        );
        if let (Ok(gnu_contents), Ok(neo_contents), Ok(gnu_state), Ok(neo_state)) = files {
            break (gnu_contents, neo_contents, gnu_state, neo_state);
        }
        assert!(
            Instant::now() < deadline,
            "GNU and Neomacs did not capture the Pydoc output buffer"
        );
        thread::sleep(Duration::from_millis(10));
    };

    expected_contents.assert_eq(&gnu_contents);
    expected_state.assert_eq(&gnu_state);
    if neo_contents != gnu_contents {
        divergences.push(format!(
            "{stage} Pydoc output buffer differs:\nGNU:\n{gnu_contents}\nNeomacs:\n{neo_contents}"
        ));
    }
    if neo_state != gnu_state {
        divergences.push(format!(
            "{stage} Pydoc buffer state differs: GNU {gnu_state:?}, Neomacs {neo_state:?}"
        ));
    }
}
