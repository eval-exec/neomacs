//! `M-x`: the other completion table the package drives.
//!
//! `C-x b` completes buffer names; `M-x` completes command names, out of the
//! obarray, with `extended-command-history` behind it and the command
//! `read-extended-command` builds its candidate list with. The candidates, the
//! prompt and the width the window gives them are all different, and the sort
//! function the package defaults to reads a command name out of the history
//! exactly as it reads a buffer name out of `buffer-name-history`.
//!
//! The fixture's commands are defined by the prelude, so the candidate list is
//! the scenario's own and not the editor's: which commands a build happens to
//! have is not something a frozen screen may depend on. One of them is run once
//! through `M-x` -- which is what puts it at the head of the history -- and the
//! session that follows has that command first and the other two ordered by
//! length and alphabet behind it.
//!
//! The screen is frozen from GNU and the candidate list is asserted against
//! GNU's, as ordinary coverage: if the two editors disagree, that is a finding
//! of its own and the failure stands.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's command prefix, which narrows `M-x` to those commands.
const PREFIX: &str = "zzz-command-";

/// The `M-x` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "M-x";

/// The fixture's commands: one name per length, so the order the default sort
/// function produces is not the alphabetical one.
const COMMANDS: [&str; 3] = ["zzz-command-bbb", "zzz-command-aaaa", "zzz-command-cc"];

/// The command the scenario runs first, which puts it at the head of
/// `extended-command-history`.
const RUN_FIRST: &str = "zzz-command-bbb";

/// What the default sort function makes of the run command and the other two:
/// the history's candidate first, the rest by length and alphabet.
const ORDER: [&str; 3] = ["zzz-command-bbb", "zzz-command-cc", "zzz-command-aaaa"];

/// The terminal this scenario renders on.
const ROWS: u16 = 24;
const COLUMNS: u16 = 80;

pub(super) fn run() {
    let prelude = default_vertico(&fixture_commands());
    let mut pair = spawn_pair("vertico-extended-command", &prelude, ROWS, COLUMNS);
    let outcome =
        catch_phase("Vertico extended command scenario", || run_body(&mut pair)).and_then(|r| r);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

/// The fixture's commands, defined as the prelude defines them: interactive,
/// and each one says on the echo area that it ran.
fn fixture_commands() -> String {
    let mut fixture = String::new();
    for command in COMMANDS {
        fixture.push_str(&format!(
            "(defun {command} ()\n  \"Say that this command ran.\"\n  (interactive)\n  \
             (message \"{command} ran\"))\n"
        ));
    }
    fixture
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
    // Run one of the fixture's commands, the way a user reaches it: through
    // `M-x`. Accepting it is what puts it at the head of the history.
    both(pair, "run a fixture command", |session| {
        invoke(session, RUN_FIRST, &format!("{RUN_FIRST} ran"));
    })?;

    both(pair, "open the M-x prompt", |session| {
        session.send_keys("M-x");
        wait_for(session, Duration::from_secs(8), "the M-x prompt", |grid| {
            grid.iter().any(|row| row.contains(PROMPT))
        });
        session.send(PREFIX.as_bytes());
        wait_for(
            session,
            Duration::from_secs(8),
            "the fixture's commands",
            |grid| candidate_rows(grid, PREFIX).len() == COMMANDS.len(),
        );
    })?;

    // The candidates are the fixture's own commands -- nothing the editor's
    // build happens to have -- in the order the package's default sort function
    // puts them: the command the history holds, then length and alphabet.
    let texts = candidate_texts(&pair.gnu.text_grid(), PREFIX);
    assert_eq!(
        texts, ORDER,
        "GNU's M-x candidates are not the fixture's commands in the sort's order"
    );
    assert_eq!(
        candidate_texts(&pair.neo.text_grid(), PREFIX),
        texts,
        "the M-x candidates differ from GNU's"
    );

    record_checkpoint(
        pair,
        "Vertico extended command",
        expect_file!["snapshots/extended_command_ansi_grid.ansi"],
        expect_file!["snapshots/extended_command_plain_grid.plain"],
    );
    Ok(())
}
