// Face colour comparison test via PTY.
//!
// Boots neomacs and GNU Emacs side-by-side, opens the Doom help
// index.org, then compares the rendered screen for coloured cells.
// Reports whether neomacs has non-default face colours matching GNU.
#![allow(dead_code)]

mod support;
use neomacs_tui_tests::*;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use support::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DoomStartupState {
    Loading,
    ConfirmingDirectoryLocalVariables,
    Ready,
}

fn doom_startup_state(grid: &[String], ready: impl Fn(&[String]) -> bool) -> DoomStartupState {
    if grid.iter().any(|row| row.contains("*Local Variables*"))
        && grid
            .iter()
            .any(|row| row.contains("Do you want to apply it?"))
    {
        DoomStartupState::ConfirmingDirectoryLocalVariables
    } else if ready(grid) {
        DoomStartupState::Ready
    } else {
        DoomStartupState::Loading
    }
}

fn wait_for_doom_startup(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    timeout: Duration,
    ready: impl Fn(&[String]) -> bool + Copy,
) {
    let deadline = Instant::now() + timeout;
    let poll_slice = Duration::from_millis(80);

    while Instant::now() < deadline {
        let gnu_state = doom_startup_state(&gnu.text_grid(), ready);
        let neo_state = doom_startup_state(&neo.text_grid(), ready);
        if gnu_state == DoomStartupState::Ready && neo_state == DoomStartupState::Ready {
            return;
        }

        match gnu_state {
            DoomStartupState::Loading => gnu.read(poll_slice),
            DoomStartupState::ConfirmingDirectoryLocalVariables => {
                // GNU files.el's `hack-local-variables-confirm' defines `y'
                // as apply-once.  Do not send `+', which persists trust in
                // the user's customization file.
                gnu.send(b"y");
                gnu.read(poll_slice);
            }
            DoomStartupState::Ready => {}
        }
        match neo_state {
            DoomStartupState::Loading => neo.read(poll_slice),
            DoomStartupState::ConfirmingDirectoryLocalVariables => {
                neo.send(b"y");
                neo.read(poll_slice);
            }
            DoomStartupState::Ready => {}
        }
    }
}

#[test]
fn index_org_has_face_colours() {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME should be set"));
    let index = [
        home.join(".emacs.d/docs/index.org"),
        home.join(".config/emacs/docs/index.org"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .expect("Doom docs/index.org should exist");
    let doom_root = index
        .parent()
        .and_then(|docs_directory| docs_directory.parent())
        .expect("Doom index should be below the Doom root");
    let docs_library = doom_root.join("lisp/lib/docs.el");
    assert!(
        docs_library.is_file(),
        "Doom docs mode library should exist at {}",
        docs_library.display()
    );
    let launch_args = [
        OsString::from("--init-directory"),
        doom_root.as_os_str().to_os_string(),
        OsString::from("--load"),
        docs_library.into_os_string(),
        index.into_os_string(),
        OsString::from("--eval=(goto-char(point-min))"),
    ];

    // Start both editors with Doom and the same document.  Opening the file at
    // launch keeps this test focused on face rendering rather than Doom keymap
    // or completion-UI differences.
    let mut gnu = TuiSession::gnu_emacs_with_init_args(launch_args.clone());
    let mut neo = TuiSession::neomacs_with_init_args(launch_args);
    let has_index = |grid: &[String]| {
        grid.iter().any(|row| row.contains("index.org"))
            && grid.iter().any(|row| row.contains("Doom Docs"))
    };
    let startup = |grid: &[String]| {
        has_index(grid)
            || grid.iter().any(|row| row.contains("Doom loaded"))
            || grid
                .iter()
                .any(|row| row.contains("Emoji images not available"))
    };
    wait_for_doom_startup(&mut gnu, &mut neo, Duration::from_secs(45), startup);
    if !startup(&gnu.text_grid()) || !startup(&neo.text_grid()) {
        dump_pair_grids("starting Doom with index.org", &gnu, &neo);
    }
    assert!(
        startup(&gnu.text_grid()),
        "GNU Doom startup did not complete"
    );
    assert!(
        startup(&neo.text_grid()),
        "Neomacs Doom startup did not complete"
    );

    // GNU may offer to download emojify images on first startup.  Decline it
    // before issuing commands so the response prompt cannot consume help keys.
    let dismiss_emojify_prompt = |session: &mut TuiSession| {
        session.read(Duration::from_secs(2));
        let prompting = session
            .text_grid()
            .iter()
            .any(|row| row.contains("Emoji images not available"));
        if prompting {
            session.send(b"n");
            session.read_until(Duration::from_secs(5), |grid| {
                !grid
                    .iter()
                    .any(|row| row.contains("Emoji images not available"))
            });
        }
    };
    dismiss_emojify_prompt(&mut gnu);
    dismiss_emojify_prompt(&mut neo);
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(20), has_index);
    if !has_index(&gnu.text_grid()) || !has_index(&neo.text_grid()) {
        dump_pair_grids("opening Doom index.org", &gnu, &neo);
    }
    assert!(
        has_index(&gnu.text_grid()),
        "GNU did not open Doom index.org"
    );
    assert!(
        has_index(&neo.text_grid()),
        "Neomacs did not open Doom index.org"
    );

    // Clear any startup warning window or message after the file action has
    // completed.  The command-line eval has already moved both buffers to top.
    send_both(&mut gnu, &mut neo, "C-g");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let has_help_title = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Doom Emacs Documentation"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), has_help_title);
    if !has_help_title(&gnu.text_grid()) || !has_help_title(&neo.text_grid()) {
        dump_pair_grids("positioning Doom index.org", &gnu, &neo);
    }
    assert!(
        has_help_title(&gnu.text_grid()),
        "GNU did not display the Doom index.org title"
    );
    assert!(
        has_help_title(&neo.text_grid()),
        "Neomacs did not display the Doom index.org title"
    );
    // Wait 3s for buffer colours to fully render after document loads
    gnu.read(Duration::from_secs(3));
    neo.read(Duration::from_secs(3));
    // Let the top-of-buffer face changes settle before comparison.
    gnu.read(Duration::from_secs(2));
    neo.read(Duration::from_secs(2));

    // Count cells with non-default foreground
    let count_colours = |sess: &TuiSession| -> (usize, usize) {
        let screen = sess.screen();
        let (rows, cols) = screen.size();
        let mut colour_fg = 0usize;
        let mut colour_any = 0usize;
        for r in 0..rows {
            for c in 0..cols {
                if let Some(cell) = screen.cell(r, c) {
                    if cell.fgcolor() != vt100::Color::Default {
                        colour_fg += 1;
                    }
                    if cell.fgcolor() != vt100::Color::Default
                        || cell.bgcolor() != vt100::Color::Default
                    {
                        colour_any += 1;
                    }
                }
            }
        }
        (colour_fg, colour_any)
    };

    let (gnu_fg, gnu_any) = count_colours(&gnu);
    let (neo_fg, neo_any) = count_colours(&neo);

    let total = gnu.screen().size().0 as usize * gnu.screen().size().1 as usize;
    eprintln!("GNU: {gnu_fg} fg-coloured / {gnu_any} any-coloured / {total} total cells");
    eprintln!("NEO: {neo_fg} fg-coloured / {neo_any} any-coloured / {total} total cells");

    // ── Identify specific face colour mismatches ──
    // Match known text patterns against their fg colours
    let gnu_s = gnu.screen();
    let neo_s = neo.screen();
    let (rows, cols) = gnu_s.size();

    // Helper: find first cell containing substring, return its fg color
    let find_fg = |screen: &vt100::Screen, needle: &str| -> Option<vt100::Color> {
        for r in 0..rows {
            let mut buf = String::new();
            for c in 0..cols {
                if let Some(cell) = screen.cell(r, c) {
                    buf.push_str(cell.contents());
                }
            }
            if buf.contains(needle) {
                // Find first column where needle starts
                let pos = buf.find(needle).unwrap_or(0);
                let cell = screen.cell(r, pos as u16);
                return cell.map(|c| c.fgcolor());
            }
        }
        None
    };

    // Check specific text patterns
    for (label, needle) in [
        ("heading + Emacs", "+ Emacs & Emacs Lisp"),
        ("link example.com", "example.com"),
        ("link gnu.org", "gnu.org"),
        ("link github", "github.com"),
        ("heading + Doom", "+ Doom Emacs"),
    ] {
        let gnu_c = find_fg(gnu_s, needle);
        let neo_c = find_fg(neo_s, needle);
        eprintln!("  {label}: gnu-fg={gnu_c:?} neo-fg={neo_c:?}");
    }

    // Compare the document palette only.  Startup warnings and the optional
    // emojify yes/no prompt live below the index.org mode line and are not
    // evidence about faces in the document under test.
    let gnu_doc_fgs = document_fg_set(gnu_s);
    let neo_doc_fgs = document_fg_set(neo_s);
    eprintln!(
        "Document fg colours: GNU={} NEO={} (target: NEO contains GNU)",
        gnu_doc_fgs.len(),
        neo_doc_fgs.len()
    );
    if !neo_doc_fgs.is_superset(&gnu_doc_fgs) {
        dump_pair_grids("index.org colours", &gnu, &neo);
        eprintln!("GNU document fg set: {gnu_doc_fgs:?}");
        eprintln!("NEO document fg set: {neo_doc_fgs:?}");
        dump_colour_rows("GNU", gnu_s, rows, cols);
        dump_colour_rows("NEO", neo_s, rows, cols);
    }
    assert!(
        neo_doc_fgs.is_superset(&gnu_doc_fgs),
        "Neomacs document palette should contain GNU's: GNU={gnu_doc_fgs:?}, NEO={neo_doc_fgs:?}"
    );
}

fn document_fg_set(screen: &vt100::Screen) -> std::collections::BTreeSet<String> {
    let (rows, cols) = screen.size();
    let mode_line_row = (0..rows)
        .find(|&row| {
            let text = screen.contents_between(row, 0, row, cols);
            text.contains("index.org") && text.contains("Doom Docs")
        })
        .expect("Doom index.org mode line should be visible");
    let mut set = std::collections::BTreeSet::new();
    for row in 0..mode_line_row {
        let text = screen.contents_between(row, 0, row, cols);
        if text.contains("File Edit Options Buffers Tools Minibuf Help") {
            continue;
        }
        for col in 0..cols {
            if let Some(cell) = screen.cell(row, col)
                && !cell.contents().trim().is_empty()
            {
                set.insert(format!("{:?}", cell.fgcolor()));
            }
        }
    }
    set
}

fn dump_colour_rows(label: &str, screen: &vt100::Screen, rows: u16, cols: u16) {
    for r in 0..rows {
        let mut text = String::new();
        let mut row_fgs = std::collections::BTreeSet::new();
        for c in 0..cols {
            if let Some(cell) = screen.cell(r, c) {
                text.push_str(cell.contents());
                if !cell.contents().trim().is_empty() {
                    row_fgs.insert(format!("{:?}", cell.fgcolor()));
                }
            }
        }
        if !text.trim().is_empty() {
            eprintln!("{label} row {r:02}: fg={row_fgs:?} |{}|", text.trim_end());
        }
    }
}
