#![cfg(unix)]
//! Issue #470: a Unicode dot in the mode line must not degrade to a literal
//! `\267` escape after initially rendering correctly.
//!
//! GNU displays a raw eight-bit character as `\` plus its octal byte
//! (src/xdisp.c:8649-8662, `%03o` of CHAR_TO_BYTE8), so a literal `\267` on
//! screen is the signature of the mode-line text having been re-derived with
//! the multibyte U+00B7 turned into a raw byte.  The regression drives real
//! PTYs: boot a GNU/neomacs pair, install a mode line containing both a
//! literal U+00B7 and `(string 183)`, then force repeated mode-line
//! re-evaluations and require the dot to stay a dot on both sides across
//! every redisplay.

use crate::support::{
    boot_pair, eval_expression, settle_session, use_backend_only_vc_mode_line,
    use_deterministic_emacs_version,
};
use neomacs_tui_tests::TuiSession;
use std::time::Duration;

const INSTALL_DOT_MODE_LINE: &str =
    "(setq mode-line-format (list \"A\" \"·\" \"B\" (string 183) \"C\"))";

fn boot_pair_plain() -> (TuiSession, TuiSession) {
    let (mut gnu, mut neo) = boot_pair("");
    settle_session(&mut gnu);
    settle_session(&mut neo);
    // Strip the VC segment so both mode lines are stable text.
    use_backend_only_vc_mode_line(&mut gnu, &mut neo);
    use_deterministic_emacs_version(&mut gnu, &mut neo);
    (gnu, neo)
}

/// Install the dot mode line and force N full mode-line re-evaluations with
/// redisplays on both sessions.
fn install_and_force_updates(gnu: &mut TuiSession, neo: &mut TuiSession, rounds: usize) {
    eval_expression(gnu, neo, INSTALL_DOT_MODE_LINE);
    for _ in 0..rounds {
        eval_expression(
            gnu,
            neo,
            "(progn (force-mode-line-update t) (redisplay) (sit-for 0))",
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn mode_line_unicode_dot_stays_a_dot_across_forced_updates() {
    let (mut gnu, mut neo) = boot_pair_plain();
    install_and_force_updates(&mut gnu, &mut neo, 5);

    // The reporter saw a CORRECT first frame and a later escaped one, so the
    // assertion must hold after arbitrarily many re-evaluations, not only on
    // the first render.  Probed on real GNU 31.1 (PTY, LC_ALL=C.UTF-8): the
    // installed mode line renders `A·B·C` -- BOTH the literal "·" and the
    // `(string 183)` element show the dot glyph, because GNU's `concat` makes
    // the integer character 183 a MULTIBYTE string holding U+00B7 (princ
    // emits c2 b7 for both), never a raw byte.  A `\267` on screen is
    // therefore the raw-byte degradation signature on either element.
    let gnu_screen = gnu.screen().contents();
    let neo_screen = neo.screen().contents();
    assert!(
        gnu_screen.contains('·'),
        "GNU lost the dot after forced updates:\n{gnu_screen}"
    );
    assert!(
        neo_screen.contains('·'),
        "neomacs degraded the dot after forced updates:\n{neo_screen}"
    );
    // Exact parity: neomacs must match GNU's `A·B·C` -- neither element may
    // degrade to its octal escape.
    assert_eq!(
        gnu_screen, neo_screen,
        "mode lines must render identically after forced updates"
    );
}
