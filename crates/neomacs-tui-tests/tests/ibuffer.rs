#![cfg(unix)]
//! TUI comparison tests: ibuffer.
//!
//! ibuffer ships with Emacs, and Neomacs carries its own copy of
//! `lisp/ibuffer.el`, so both editors run the same Lisp here: this file
//! belongs with the bundled-Emacs pair tests, not with the package-oracle
//! machinery in `tests/package_tui/`. A divergence in these screens is
//! therefore the display engine, or a drifted file under Neomacs's `lisp/`.
//!
//! Fixture rules, which the whole file depends on:
//!
//! * the buffer set is created by the fixture, never observed from the host:
//!   *scratch* and *Messages* are the two buffers both editors always have,
//!   and the fixture adds exactly the named buffers below;
//! * no buffer reads the clock, the locale, the user name or any installed
//!   tool: buffer names and contents are literals fixed here;
//! * the one file-visiting buffer visits a shared temporary path, so the
//!   File column shows one and the same path in both editors rather than a
//!   per-session one;
//! * the terminal is always booted at the harness default size and narrowed
//!   afterwards, because a narrow boot can wrap the startup echo-area message
//!   and break the geometry the grid is compared on.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Fixtures ───────────────────────────────────────────────

/// The buffers every scenario but the many-buffer one runs against.
///
/// `alpha-notes` is modified (so its row carries `*`), `delta-read-only` is
/// read-only (so its row carries `%`), `gamma-lisp` and `beta-report` are
/// unmodified and in a non-default major mode, `epsilon-...` is longer than
/// the name column and must be elided, `zeta-日本語` is wide without being
/// long, `eta-...` is both wide and long (so the elide lands inside
/// double-width cells), and `visited.txt` is the only buffer with a file.
const FIXTURE_BUFFERS: usize = 8;

/// `{visited}` is the shared file the single file-visiting buffer visits.
fn fixture_source() -> &'static str {
    ";;; ibuffer-fixture.el --- deterministic buffer set  -*- lexical-binding: t; -*-\n\
     (progn\n\
     \x20 (with-current-buffer (get-buffer-create \"alpha-notes\")\n\
     \x20   (insert \"alpha one\\nalpha two\\n\"))\n\
     \x20 (with-current-buffer (get-buffer-create \"beta-report\")\n\
     \x20   (text-mode)\n\
     \x20   (insert \"beta body line\\n\")\n\
     \x20   (set-buffer-modified-p nil))\n\
     \x20 (with-current-buffer (get-buffer-create \"gamma-lisp\")\n\
     \x20   (emacs-lisp-mode)\n\
     \x20   (insert \"(defun gamma ()\\n  42)\\n\")\n\
     \x20   (set-buffer-modified-p nil))\n\
     \x20 (with-current-buffer (get-buffer-create \"delta-read-only\")\n\
     \x20   (insert \"delta read only\\n\")\n\
     \x20   (set-buffer-modified-p nil)\n\
     \x20   (setq buffer-read-only t))\n\
     \x20 (with-current-buffer (get-buffer-create \"epsilon-with-a-very-long-buffer-name-for-reflow\")\n\
     \x20   (insert \"epsilon\\n\")\n\
     \x20   (set-buffer-modified-p nil))\n\
     \x20 (with-current-buffer (get-buffer-create \"zeta-\u{65e5}\u{672c}\u{8a9e}\")\n\
     \x20   (insert \"zeta body\\n\")\n\
     \x20   (set-buffer-modified-p nil))\n\
     \x20 (with-current-buffer (get-buffer-create \"eta-\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30d0}\u{30c3}\u{30d5}\u{30a1}\u{540d}\u{306f}\u{3068}\u{3066}\u{3082}\u{9577}\u{3044}\u{3067}\u{3059}\")\n\
     \x20   (insert \"eta body\\n\")\n\
     \x20   (set-buffer-modified-p nil))\n\
     \x20 (find-file-noselect \"{visited}\"))\n"
}

/// Forty fixture buffers, all unmodified, to make the list outgrow its window.
fn many_buffers_source() -> String {
    let mut source =
        String::from(";;; ibuffer-many.el --- many buffers  -*- lexical-binding: t; -*-\n(progn\n");
    for index in 0..40 {
        source.push_str(&format!(
            "  (with-current-buffer (get-buffer-create \"many-{index:02}\")\n    (insert \"row {index:02}\\n\")\n    (set-buffer-modified-p nil))\n"
        ));
    }
    source.push_str(")\n");
    source
}

/// Boot both editors with `source` loaded, and the file it visits on disk.
fn boot_fixture(source: &str) -> (TuiSession, TuiSession, TuiTempFile) {
    let visited = TuiTempFile::new(
        "neomacs-ibuffer-visited-",
        "visited.txt",
        "visited body line\n",
    );
    let source = source.replace("{visited}", &visited.display().to_string());
    let init = TuiTempFile::new("neomacs-ibuffer-fixture-", "ibuffer-fixture.el", &source);
    let extra = format!("-l {}", init.display());
    let (gnu, neo) = boot_pair(&extra);
    (gnu, neo, visited)
}

fn boot_ibuffer_fixture() -> (TuiSession, TuiSession, TuiTempFile) {
    boot_fixture(fixture_source())
}

// ── Pair helpers ───────────────────────────────────────────

/// Wait for `predicate` on both editors, then drain the frame it paints.
fn step(gnu: &mut TuiSession, neo: &mut TuiSession, predicate: impl Fn(&[String]) -> bool + Copy) {
    gnu.read_until(Duration::from_secs(8), predicate);
    neo.read_until(Duration::from_secs(12), predicate);
    read_both(gnu, neo, Duration::from_millis(500));
}

/// Invoke ibuffer and wait for the fixture's list to be on screen.
fn open_ibuffer(gnu: &mut TuiSession, neo: &mut TuiSession) {
    invoke_mx_command(gnu, neo, "ibuffer");
    step(gnu, neo, |grid| {
        grid.iter().any(|row| row.contains("[ Default ]"))
            && grid.iter().any(|row| row.contains("alpha-notes"))
    });
}

/// Assert `predicate` against each editor's own grid, naming it on failure.
fn assert_both(
    what: &str,
    gnu: &TuiSession,
    neo: &TuiSession,
    predicate: impl Fn(&[String]) -> bool,
) {
    for (label, session) in [("GNU", gnu), ("Neomacs", neo)] {
        let grid = session.text_grid();
        assert!(
            predicate(&grid),
            "{label} should {what}; its grid was:\n{}",
            grid.join("\n")
        );
    }
}

fn row_starting_with<'a>(grid: &'a [String], prefix: &str) -> Option<&'a String> {
    grid.iter().find(|row| row.starts_with(prefix))
}

// ── Tests ──────────────────────────────────────────────────

#[test]
fn ibuffer_list_shows_name_size_mode_file_columns_and_group_line() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    assert_both(
        "render the column titles over their underlines",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("Name") && row.contains("Size") && row.contains("Mode"))
                && grid.iter().any(|row| row.starts_with(" --- ----"))
        },
    );
    assert_both(
        "open the list with a Default filter group",
        &gnu,
        &neo,
        |grid| grid.iter().any(|row| row.contains("[ Default ]")),
    );
    // Size 20 with the modified char, Text for the file-less text buffer,
    // Elisp/d for emacs-lisp-mode, % for the read-only buffer.
    assert_both(
        "show each fixture buffer with its size and mode",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains(" *   alpha-notes               20 Fundamental"))
                && grid
                    .iter()
                    .any(|row| row.contains("     beta-report               15 Text"))
                && grid
                    .iter()
                    .any(|row| row.contains("     gamma-lisp                22 Elisp/d"))
                && grid
                    .iter()
                    .any(|row| row.contains("  %  delta-read-only           16 Fundamental"))
        },
    );
    assert_both(
        "elide the name that outgrows the name column and keep the wide names",
        &gnu,
        &neo,
        |grid| {
            grid.iter().any(|row| row.contains("epsilon-with-a-..."))
                && grid
                    .iter()
                    .any(|row| row.contains("zeta-\u{65e5}\u{672c}\u{8a9e}"))
                && grid.iter().any(|row| row.contains("eta-"))
        },
    );
    assert_both(
        "name the file the one file-visiting buffer visits",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("18 Text") && row.trim_end().ends_with("visited.txt"))
        },
    );
    assert_both("summarize the whole list below it", &gnu, &neo, |grid| {
        let total = 2 + FIXTURE_BUFFERS;
        grid.iter().any(|row| {
            row.contains(&format!("{total} buffers")) && row.contains("1 file, no processes")
        })
    });

    assert_pair_exact_display(
        "ibuffer_list_shows_name_size_mode_file_columns_and_group_line",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_marks_show_marked_and_deletion_chars_and_unmarking_clears_them() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    // Point starts on the first buffer row: step onto *Messages*, mark it
    // (`>`), then mark the next line for deletion (`D`).
    send_both(&mut gnu, &mut neo, "n m d");
    step(&mut gnu, &mut neo, |grid| {
        row_starting_with(grid, "D").is_some() && row_starting_with(grid, ">*").is_some()
    });
    assert_both(
        "mark the current line with > and the next one with D",
        &gnu,
        &neo,
        |grid| {
            row_starting_with(grid, ">*%  *Messages*").is_some()
                && row_starting_with(grid, "D*   alpha-notes").is_some()
        },
    );

    // Back onto the deletion mark and unmark it.
    send_both(&mut gnu, &mut neo, "p u");
    step(&mut gnu, &mut neo, |grid| {
        row_starting_with(grid, "D").is_none()
    });
    assert_both(
        "clear the deletion mark the cursor is on",
        &gnu,
        &neo,
        |grid| {
            row_starting_with(grid, "D").is_none()
                && row_starting_with(grid, ">*%  *Messages*").is_some()
        },
    );

    // `t` inverts every mark, so the one marked line becomes blank and all
    // the others become marked.
    send_both(&mut gnu, &mut neo, "t");
    step(&mut gnu, &mut neo, |grid| {
        row_starting_with(grid, ">    *scratch*").is_some()
            && row_starting_with(grid, " *%  *Messages*").is_some()
    });
    assert_both("invert every mark on t", &gnu, &neo, |grid| {
        row_starting_with(grid, ">    *scratch*").is_some()
            && row_starting_with(grid, " *%  *Messages*").is_some()
            && row_starting_with(grid, ">*   alpha-notes").is_some()
    });

    // `U` clears them all again.
    send_both(&mut gnu, &mut neo, "U");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().all(|row| !row.starts_with(">"))
    });
    assert_both("clear every mark on U", &gnu, &neo, |grid| {
        grid.iter().all(|row| !row.starts_with(">"))
            && row_starting_with(grid, "     *scratch*").is_some()
    });

    assert_pair_exact_display(
        "ibuffer_marks_show_marked_and_deletion_chars_and_unmarking_clears_them",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_delete_marks_confirm_and_kill_the_marked_buffers() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "n d d");
    step(&mut gnu, &mut neo, |grid| {
        row_starting_with(grid, "D").is_some()
    });

    // Two marked buffers make ibuffer ask through its confirmation buffer,
    // displayed in its own window.
    send_both(&mut gnu, &mut neo, "x");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter()
            .any(|row| row.contains("Really kill 2 buffers?"))
    });
    assert_both("ask before killing two buffers", &gnu, &neo, |grid| {
        grid.iter()
            .any(|row| row.contains("*Ibuffer confirmation*"))
            && grid
                .iter()
                .any(|row| row.contains("Really kill 2 buffers? (y or n)"))
    });

    send_both(&mut gnu, &mut neo, "y");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("killed 2 buffers"))
    });
    assert_both(
        "drop the killed buffers from the list",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("Operation finished; killed 2 buffers"))
                && !grid.iter().any(|row| row.contains("alpha-notes"))
                && !grid.iter().any(|row| row.contains("*Messages*"))
                && grid.iter().any(|row| row.contains("8 buffers"))
        },
    );

    assert_pair_exact_display(
        "ibuffer_delete_marks_confirm_and_kill_the_marked_buffers",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_sort_by_name_size_and_recency_reorders_the_list() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "s a");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("by alphabetic"))
    });
    assert_both("sort the list by name", &gnu, &neo, |grid| {
        grid.iter()
            .any(|row| row.contains("(IBuffer by alphabetic)"))
            && grid.iter().any(|row| row.contains("*Messages*"))
            && grid.iter().any(|row| row.contains("*scratch*"))
    });

    send_both(&mut gnu, &mut neo, "s s");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("by size"))
    });
    assert_both("sort the list by size", &gnu, &neo, |grid| {
        grid.iter().any(|row| row.contains("(IBuffer by size)"))
    });

    send_both(&mut gnu, &mut neo, "s v");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("by recency"))
    });
    assert_both("sort the list by recency", &gnu, &neo, |grid| {
        grid.iter().any(|row| row.contains("(IBuffer by recency)"))
    });

    assert_pair_exact_display(
        "ibuffer_sort_by_name_size_and_recency_reorders_the_list",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_filter_by_name_adds_the_filter_line_and_shrinks_the_list() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "/ n");
    step(&mut gnu, &mut neo, |grid| {
        grid.last()
            .is_some_and(|row| row.contains("Filter by name (regexp):"))
    });
    assert_both("prompt for a name filter", &gnu, &neo, |grid| {
        grid.last()
            .is_some_and(|row| row.contains("Filter by name (regexp):"))
    });

    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha");
    }
    send_both(&mut gnu, &mut neo, "RET");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("[buffer name: alpha]"))
    });
    // The filter shows up as the header line above the list, and the list
    // and its summary shrink to the surviving buffer.
    assert_both(
        "show the active filter in the header line",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("IBuffer by recency [buffer name: alpha]"))
                && grid.iter().any(|row| row.contains("alpha-notes"))
                && !grid.iter().any(|row| row.contains("beta-report"))
                && grid.iter().any(|row| row.contains("1 buffer"))
        },
    );

    // `/ /` disables the filter stack again, so the header line goes away and
    // the whole list comes back.
    send_both(&mut gnu, &mut neo, "/ /");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("beta-report"))
            && !grid.iter().any(|row| row.contains("[buffer name: alpha]"))
    });
    assert_both(
        "restore the whole list when filtering is disabled",
        &gnu,
        &neo,
        |grid| {
            grid.iter().any(|row| row.contains("beta-report"))
                && !grid.iter().any(|row| row.contains("[buffer name: alpha]"))
        },
    );

    assert_pair_exact_display(
        "ibuffer_filter_by_name_adds_the_filter_line_and_shrinks_the_list",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_filter_group_line_names_the_buffers_a_filter_selects() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "/ n alpha");
    send_both(&mut gnu, &mut neo, "RET");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("[buffer name: alpha]"))
    });

    // `/ g` turns the active filter into a named group, which the list then
    // renders as its own `[ name ]` section above Default.
    send_both(&mut gnu, &mut neo, "/ g");
    step(&mut gnu, &mut neo, |grid| {
        grid.last()
            .is_some_and(|row| row.contains("Name for filtering group:"))
    });
    for session in [&mut gnu, &mut neo] {
        session.send(b"greek");
    }
    send_both(&mut gnu, &mut neo, "RET");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("[ greek ]"))
    });
    assert_both("render the named group above Default", &gnu, &neo, |grid| {
        let group = grid.iter().position(|row| row.contains("[ greek ]"));
        let default = grid.iter().position(|row| row.contains("[ Default ]"));
        group.is_some()
            && default.is_some()
            && group < default
            && grid.iter().any(|row| row.contains("alpha-notes"))
            && !grid.iter().any(|row| row.contains("[buffer name: alpha]"))
    });

    assert_pair_exact_display(
        "ibuffer_filter_group_line_names_the_buffers_a_filter_selects",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_revert_g_regenerates_the_list_and_keeps_point() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "n m n");
    step(&mut gnu, &mut neo, |grid| {
        row_starting_with(grid, ">*").is_some()
    });

    send_both(&mut gnu, &mut neo, "g");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter()
            .any(|row| row.contains("Updating buffer list...done"))
    });
    assert_both(
        "keep the marks and the list across a revert",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("Updating buffer list...done"))
                && row_starting_with(grid, ">*%  *Messages*").is_some()
                && grid.iter().any(|row| row.contains("alpha-notes"))
        },
    );

    assert_pair_exact_display(
        "ibuffer_revert_g_regenerates_the_list_and_keeps_point",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_ret_visits_the_buffer_on_the_current_line() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    open_ibuffer(&mut gnu, &mut neo);

    // Four rows down from the first buffer is gamma-lisp, whose body then
    // has to replace the list.
    send_both(&mut gnu, &mut neo, "n n n n");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("gamma-lisp"))
    });
    send_both(&mut gnu, &mut neo, "RET");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("(defun gamma ()"))
            && !grid.iter().any(|row| row.contains("[ Default ]"))
    });
    assert_both("visit the buffer on the current line", &gnu, &neo, |grid| {
        grid.iter().any(|row| row.contains("(defun gamma ()"))
            && grid.iter().any(|row| row.contains("  42)"))
            && grid.iter().any(|row| row.contains("gamma-lisp"))
    });

    assert_pair_exact_display(
        "ibuffer_ret_visits_the_buffer_on_the_current_line",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_list_scrolls_its_window_over_more_buffers_than_it_holds() {
    let (mut gnu, mut neo, _visited) = boot_fixture(&many_buffers_source());
    resize_both(&mut gnu, &mut neo, 24, 80);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    open_ibuffer(&mut gnu, &mut neo);

    step(&mut gnu, &mut neo, |grid| {
        row_starting_with(grid, "     many-00").is_some()
    });
    assert_both(
        "show the top of the list in a twenty-row window",
        &gnu,
        &neo,
        |grid| {
            row_starting_with(grid, "     many-00").is_some()
                && grid.iter().any(|row| row.contains("Top L4"))
        },
    );

    send_both(&mut gnu, &mut neo, "M->");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.contains("Bot L"))
    });
    assert_both("scroll to the end of the list", &gnu, &neo, |grid| {
        grid.iter().any(|row| row.contains("many-39"))
            && grid.iter().any(|row| row.contains("Bot L"))
            && grid.iter().any(|row| row.contains("42 buffers"))
    });

    assert_pair_exact_display(
        "ibuffer_list_scrolls_its_window_over_more_buffers_than_it_holds",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_columns_reflow_in_a_narrow_terminal_with_wide_names() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    resize_both(&mut gnu, &mut neo, 24, 80);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    open_ibuffer(&mut gnu, &mut neo);

    assert_both(
        "keep the columns aligned once the wide-name rows are elided",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("zeta-\u{65e5}\u{672c}\u{8a9e}"))
                && grid
                    .iter()
                    .any(|row| row.contains("eta-\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30d0}"))
        },
    );
    assert_pair_exact_display(
        "ibuffer_columns_reflow_in_a_narrow_terminal_with_wide_names/80",
        &gnu,
        &neo,
    );

    // Narrower still: the rows that no longer fit end in the truncation
    // glyph, including the one carrying the wide name.
    resize_both(&mut gnu, &mut neo, 24, 60);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    step(&mut gnu, &mut neo, |grid| {
        grid.iter().any(|row| row.ends_with('$'))
    });
    assert_both("truncate the rows that no longer fit", &gnu, &neo, |grid| {
        grid.iter()
            .any(|row| row.contains("Mode             Filename$"))
            && grid.iter().any(|row| row.ends_with('$'))
    });

    assert_pair_exact_display(
        "ibuffer_columns_reflow_in_a_narrow_terminal_with_wide_names/60",
        &gnu,
        &neo,
    );
}

#[test]
fn ibuffer_alternate_format_shows_the_full_long_and_wide_names() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    resize_both(&mut gnu, &mut neo, 24, 80);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    open_ibuffer(&mut gnu, &mut neo);

    // The second entry of `ibuffer-formats' drops the size and mode columns,
    // so the long names are no longer elided -- including the one that is
    // wider than it is long.
    send_both(&mut gnu, &mut neo, "`");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter()
            .any(|row| row.contains("Name") && !row.contains("Size"))
    });
    assert_both(
        "show the unelided names in the alternate format",
        &gnu,
        &neo,
        |grid| {
            grid.iter().any(|row| row.contains("epsilon-with-a-very-long-buffer-name-for-reflow"))
            && grid
                .iter()
                .any(|row| row.contains("eta-\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30d0}\u{30c3}\u{30d5}\u{30a1}\u{540d}\u{306f}\u{3068}\u{3066}\u{3082}\u{9577}\u{3044}\u{3067}\u{3059}"))
            && grid.iter().any(|row| row.contains("10 buffers"))
        },
    );

    assert_pair_exact_display(
        "ibuffer_alternate_format_shows_the_full_long_and_wide_names",
        &gnu,
        &neo,
    );
}

/// The truncation glyph over a double-width cell at the window's right edge.
///
/// The alternate format prints `eta-...` in full, so a forty-column window
/// cuts its row inside the last wide character. GNU paints `$` in the two
/// cells that half-glyph leaves; Neomacs paints a space in the first of them
/// and `$` in the last, so the row differs by one cell.
///
/// This is a real divergence and the test is meant to stay RED: it is not to
/// be blessed from Neomacs, weakened or skipped. Nothing about it is
/// specific to `ibuffer-formats`: the same half-cell is reachable through the
/// default format by narrowing until the elided wide name reaches the edge
/// (nineteen columns), and it shows up again in the echo area when ibuffer's
/// own `Filter by buffer name added: ...` message wraps a wide character (a
/// twenty-four by thirty-six terminal) -- GNU paints its continuation glyph
/// there, Neomacs a space.
#[test]
fn ibuffer_truncated_wide_name_at_the_window_edge() {
    let (mut gnu, mut neo, _visited) = boot_ibuffer_fixture();
    resize_both(&mut gnu, &mut neo, 24, 40);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    open_ibuffer(&mut gnu, &mut neo);

    // The default format keeps every wide name well inside the window, so it
    // agrees; the wide name only reaches the edge in the alternate format.
    assert_both(
        "elide the wide name inside the window at forty columns",
        &gnu,
        &neo,
        |grid| {
            grid.iter()
                .any(|row| row.contains("eta-\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30d0}"))
        },
    );
    assert_pair_exact_display(
        "ibuffer_truncated_wide_name_at_the_window_edge/elided",
        &gnu,
        &neo,
    );

    send_both(&mut gnu, &mut neo, "`");
    step(&mut gnu, &mut neo, |grid| {
        grid.iter()
            .any(|row| row.contains("Name") && !row.contains("Size"))
    });
    assert_both(
        "print the wide name up to the last columns of the window",
        &gnu,
        &neo,
        |grid| {
            grid.iter().any(|row| {
                row.contains("eta-\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30d0}") && row.ends_with('$')
            })
        },
    );

    // Prints both editors' full screens before the comparison so a teammate
    // sees the one cell that differs in context.
    dump_pair_grids("ibuffer_truncated_wide_name_at_the_window_edge", &gnu, &neo);
    assert_pair_exact_display("ibuffer_truncated_wide_name_at_the_window_edge", &gnu, &neo);
}
