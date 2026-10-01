//! Candidates wider than the window: what the last column holds.
//!
//! A terminal has a glyph for a line that does not fit the window -- `\` when
//! the line continues on the next row, `$` when the window cuts it -- and the
//! cell is the screen's own statement that there is more text than the row
//! shows. Neomacs fills that cell with the candidate's text instead and shows
//! neither glyph nor continuation, so a candidate longer than the window is
//! drawn differently in the two editors even though the same characters lead up
//! to it.
//!
//! The scenario's fixture has one such candidate per kind of width the terminal
//! has to count in: a plain ASCII name, a name of double-width CJK characters,
//! whose cut can land in the middle of a character, and a name of combining
//! marks, which take no column of their own. Each is frozen on its own --
//! narrowing the input to one candidate at a time -- because what the last
//! column holds is a claim about the character before it, and one screen
//! holding all three would not say which line is wrong.
//!
//! The screens are also where a second difference shows: the wrapped candidate
//! lines make GNU's minibuffer window taller than Neomacs', so the main
//! window's mode line is two rows higher up in GNU's screens than in Neomacs'.
//! Both differences are reported and neither is frozen from Neomacs.
//!
//! Every screen is blessed from GNU. The comparisons against Neomacs are what
//! fail, and that failure is the point of this scenario: it is not to be
//! re-blessed away, and none of these screens must be re-blessed from Neomacs.

use std::time::Duration;

use expect_test::{ExpectFile, expect_file};

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "truncation-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The width of the terminal this scenario renders on: narrower than any of the
/// fixture's names, so every candidate row is cut.
const COLUMNS: usize = 40;

/// The terminal the pair is spawned on before it is narrowed.
///
/// A pair is spawned at a geometry the harness's own checkpoint can see: it
/// waits for the main window's mode line two rows above the screen's last row,
/// which is only where an echo area one row tall puts it. A terminal this
/// narrow wraps the echo area's startup message onto two rows, so the pair is
/// spawned wide and narrowed before the first session opens.
const WIDE: (u16, u16) = (30, 100);

/// The height the narrowed terminal keeps.
const ROWS: u16 = 24;

/// The glyph a terminal puts in the last column of a line that does not fit.
///
/// `\\` continues on the next row; `$` is what a line the window *cuts* ends
/// with. The characters a terminal has for the two are its own, and this is the
/// one this terminal uses.
const LINE_GLYPH: char = '\\';

/// The fixture's candidate count.
const TOTAL: usize = 3;

/// The fixture: one name per kind of width a candidate can be cut in.
///
/// The names are written out here in full -- one of double-width CJK
/// characters, one of base characters carrying combining accents, which take
/// one column together -- so the fixture is fixed and nothing in it is derived
/// from the host.
const FIXTURE: &str = r#"
(dolist (name (list "truncation-abcdefghijklmnopqrstuvwxyz-abcdefghijklmnopqrstuvwxyz"
                    "truncation-日本語日本語日本語日本語日本語"
                    "truncation-áááááááááááááááááááááááááááááááá"))
  (with-current-buffer (get-buffer-create name)
    (erase-buffer)
    (insert (concat name "\n"))))
"#;

/// The CJK character the fixture's second candidate is made of, and the
/// combining mark its third one carries.
const CJK_CHAR: char = '\u{65e5}';
const COMBINING_MARK: char = '\u{0301}';

/// What narrows the input to the CJK candidate, and to the combining one.
const CJK_ONLY: &str = "\u{65e5}";
const COMBINING_ONLY: &str = "a\u{0301}";

pub(super) fn run() {
    let prelude = default_vertico(FIXTURE);
    let mut pair = spawn_pair("vertico-truncation", &prelude, WIDE.0, WIDE.1);
    let outcome =
        catch_phase("Vertico truncation scenario", || run_body(&mut pair)).and_then(|r| r);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
    // The first session opens at the wide geometry and the terminal is narrowed
    // under it: a pair is spawned at a geometry the harness's own checkpoint can
    // see, and narrowing it first would wrap that checkpoint's echo area.
    open_session(pair, "candidate cut in ASCII", "");
    pair.resize_both(
        ROWS,
        u16::try_from(COLUMNS).expect("the width fits a terminal"),
    );
    pair.settle_both(Duration::from_secs(2));

    // Each screen is blessed on its own, so a divergence found on the first one
    // does not leave the ones behind it unrecorded: the phases below run under
    // their own `catch_phase` and their failures are reported together.
    let mut failures = Vec::new();
    for Phase {
        description,
        input,
        marker,
        ansi,
        plain,
    } in phases()
    {
        let outcome = catch_phase(description, || {
            open_session(pair, description, input);
            assert_gnu_line_glyph(pair, description, marker);
            record_checkpoint(
                pair,
                &format!("Vertico truncation {description}"),
                ansi,
                plain,
            );
        });
        if let Err(failure) = outcome {
            failures.push(failure);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// One screen: the input that narrows the fixture to the candidate it is about,
/// and the frozen files that screen is blessed into.
struct Phase {
    description: &'static str,
    /// What is typed after the fixture's prefix. Empty for the first phase, so
    /// that screen holds every candidate the fixture has.
    input: &'static str,
    /// How a candidate row this phase is about is told apart from the others.
    marker: fn(&str) -> bool,
    ansi: ExpectFile,
    plain: ExpectFile,
}

/// The three screens, in the order they are recorded.
fn phases() -> Vec<Phase> {
    vec![
        Phase {
            description: "candidate cut in ASCII",
            input: "",
            marker: |row| row.contains("abcdefghijklmnopqrstuvwxyz"),
            ansi: expect_file!["snapshots/truncation_ascii_ansi_grid.ansi"],
            plain: expect_file!["snapshots/truncation_ascii_plain_grid.plain"],
        },
        Phase {
            description: "candidate cut in double-width characters",
            input: CJK_ONLY,
            marker: |row| row.contains(CJK_CHAR),
            ansi: expect_file!["snapshots/truncation_wide_ansi_grid.ansi"],
            plain: expect_file!["snapshots/truncation_wide_plain_grid.plain"],
        },
        Phase {
            description: "candidate cut with combining marks",
            input: COMBINING_ONLY,
            marker: |row| row.contains(COMBINING_MARK),
            ansi: expect_file!["snapshots/truncation_combining_ansi_grid.ansi"],
            plain: expect_file!["snapshots/truncation_combining_plain_grid.plain"],
        },
    ]
}

/// Open a completion session holding the candidates `phase` is about.
///
/// Every phase starts its own session: each phase's input narrows the fixture
/// to one candidate, so it only means anything typed from the prefix alone.
fn open_session(pair: &mut PackageTuiPair, description: &str, input: &str) {
    let _ = description;
    both(pair, "abandon the previous session", |session| {
        session.send_key("C-g");
        wait_for(
            session,
            Duration::from_secs(8),
            "the minibuffer to close",
            |grid| !grid.iter().any(|row| row.contains(PROMPT)),
        );
    })
    .expect("both peers close the previous session");
    both(pair, "open switch-to-buffer", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "the switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains(PROMPT)),
        );
        session.send(format!("{PREFIX}{input}").as_bytes());
        wait_for(
            session,
            Duration::from_secs(8),
            "the fixture's candidates",
            |grid| (1..=TOTAL).contains(&candidate_rows(grid, PREFIX).len()),
        );
    })
    .expect("both peers open the session");
}

/// GNU puts the terminal's glyph for a line that does not fit in the last
/// column of the candidate row this phase is about, and continues the line on
/// the row below it.
///
/// A terminal has one glyph for each way a line can fail to fit: `\` when the
/// line continues on the next row, `$` when it is cut where the window ends.
/// Which one a screen shows is the window's own `truncate-lines`, and the
/// assertion below is that GNU's row carries *a* glyph in that column rather
/// than the text of the candidate: Neomacs leaves the column to the text and
/// shows neither glyph nor continuation.
fn assert_gnu_line_glyph(pair: &PackageTuiPair, description: &str, marker: fn(&str) -> bool) {
    let grid = pair.gnu.text_grid();
    let rows: Vec<u16> = candidate_rows(&grid, PREFIX)
        .into_iter()
        .filter(|row| marker(&grid[usize::from(*row)]))
        .collect();
    assert!(
        !rows.is_empty(),
        "GNU shows no candidate for {description}:\n{}",
        grid.join("\n")
    );
    for row in rows {
        let drawn: Vec<char> = grid[usize::from(row)].chars().collect();
        assert_eq!(
            drawn.get(COLUMNS - 1).copied(),
            Some(LINE_GLYPH),
            "GNU's candidate row {row} does not carry the glyph for a line that \
             does not fit in its last column, for {description}: {:?}",
            grid[usize::from(row)]
        );
        let continuation = grid
            .get(usize::from(row) + 1)
            .map(|row| row.trim().to_owned())
            .unwrap_or_default();
        assert!(
            !continuation.is_empty(),
            "GNU's candidate row {row} does not continue on the row below it, for \
             {description}:\n{}",
            grid.join("\n")
        );
    }
}
