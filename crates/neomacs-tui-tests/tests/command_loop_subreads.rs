//! TUI comparison tests for the interactive command classes that exercise the
//! command loop's sub-reads, `this-command-keys`, prefix-arg, and quit
//! machinery — the same machinery this session's event-loop commits touched
//! (3a57958a0 unbound-keys, be35ff7d9 deactivate-mark, 621e5eaf0 single-C-g,
//! bbd87e4e6 this-command-keys, ab64604e0 wait-loop maybe_quit).
//!
//! The existing TUI suite covers registers/bookmarks and query-replace (the
//! commands that regressed once and were fixed in bbd87e4e6), but has ZERO
//! coverage of the *other* commands in the same class:
//!
//!   1. Keyboard macros — `C-x ( … C-x )` define then `C-x e` / `e` replay,
//!      and a macro that contains a read-char command. Macros RECORD
//!      `this-command-keys`, so they are the highest collateral risk of the
//!      `this-command-keys` fix.
//!   2. Read-a-char-mid-command — `zap-to-char` (`M-z`), `quoted-insert`
//!      (`C-q` octal / `C-q` literal / `C-q TAB`). These read via
//!      read-char / read-char-from-minibuffer like the register bug.
//!   3. isearch — `C-s foo`, `C-s C-s` repeat, `RET` exit, `C-g` abort.
//!   4. Recursive minibuffers — `enable-recursive-minibuffers`, `M-:` then
//!      `M-x` nested (the save/restore path bbd87e4e6 added).
//!   5. Prefix args into reading commands — `C-u C-q`, `M-5 C-x e`, `C-u M-z`.
//!   6. `C-x z` (repeat).
//!   7. Unbound key mid-sequence + single-`C-g` quitting a sub-read.
//!
//! Each test feeds identical keystrokes to GNU Emacs and Neomacs in parallel
//! PTYs and asserts the rendered vt100 grids match (the established pattern in
//! registers_bookmarks.rs / replace_sort.rs).

mod support;
use neomacs_tui_tests::{StrictGridOptions, TuiSession, assert_grids_strict};
use std::time::Duration;
use support::*;

/// Compare every terminal cell across the complete PTY width and height.
/// There are deliberately no ignored rows, masks, or mismatch allowances:
/// every divergence from the GNU oracle must fail at its exact cell.
fn assert_pair_strict(label: &str, gnu: &TuiSession, neo: &TuiSession) {
    assert_grids_strict(
        label,
        gnu.screen(),
        neo.screen(),
        &StrictGridOptions::default(),
    );
}

// ── 1. Keyboard macros ─────────────────────────────────────────────────

/// Define a macro with `C-x (`, type `abc`, close with `C-x )`, then replay it
/// once with `C-x e` and a second time with a bare `e`. The buffer should hold
/// the original `abc` plus two replays (`abcabcabc`). Macros record
/// `this-command-keys`, so a stale-this-command-keys bug would replay the
/// wrong keys or drop characters — the highest collateral risk of the
/// `this-command-keys` fix (bbd87e4e6).
#[test]
fn kbd_macro_define_and_replay_with_cx_e_then_e() {
    let (mut gnu, mut neo) = boot_pair("");

    // Define: C-x ( a b c C-x )
    send_both(&mut gnu, &mut neo, "C-x (");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "a b c");
    send_both(&mut gnu, &mut neo, "C-x )");
    let defined = |grid: &[String]| grid.iter().any(|row| row.contains("abc"));
    gnu.read_until(Duration::from_secs(6), defined);
    neo.read_until(Duration::from_secs(8), defined);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    // Replay once with C-x e, then again with bare e (e repeats the macro).
    send_both(&mut gnu, &mut neo, "C-x e");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    send_both(&mut gnu, &mut neo, "e");

    let replayed = |grid: &[String]| grid.iter().any(|row| row.contains("abcabcabc"));
    gnu.read_until(Duration::from_secs(6), replayed);
    neo.read_until(Duration::from_secs(8), replayed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict("kbd_macro_define_and_replay_with_cx_e_then_e", &gnu, &neo);
}

/// A macro that *contains a read-char command*: define `M-z x` (zap-to-char x)
/// inside a macro, then replay it. Replaying must read the recorded `x` from
/// the macro's event stream (not prompt for a fresh char), zapping to the next
/// `x`. This is the read-char-mid-macro path: `read-char-from-minibuffer`
/// inside `execute-kbd-macro` must consume the macro-recorded char, exactly
/// the read-key/this-command-keys interaction the register bug lived in.
#[test]
fn kbd_macro_containing_zap_to_char_replays_recorded_char() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "macro-zap.txt", "axbxcxd\n", "C-x C-f");

    // Point at buffer start, define a macro: C-x ( M-z x C-x )
    send_both(&mut gnu, &mut neo, "M-<");
    send_both(&mut gnu, &mut neo, "C-x (");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // M-z reads a char from the minibuffer ("Zap to char:"); send x.
    send_both(&mut gnu, &mut neo, "M-z");
    let zap_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Zap to char:"));
    gnu.read_until(Duration::from_secs(6), zap_prompt);
    neo.read_until(Duration::from_secs(8), zap_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "x");
    send_both(&mut gnu, &mut neo, "C-x )");
    // After defining: "axbxcxd" -> first M-z x kills "ax", leaving "bxcxd".
    let after_define = |grid: &[String]| grid.iter().any(|row| row.contains("bxcxd"));
    gnu.read_until(Duration::from_secs(6), after_define);
    neo.read_until(Duration::from_secs(8), after_define);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    // Replay: should zap to the next recorded 'x', killing "bx" -> "cxd".
    send_both(&mut gnu, &mut neo, "C-x e");
    let after_replay = |grid: &[String]| grid.iter().any(|row| row.contains("cxd"));
    gnu.read_until(Duration::from_secs(6), after_replay);
    neo.read_until(Duration::from_secs(8), after_replay);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict(
        "kbd_macro_containing_zap_to_char_replays_recorded_char",
        &gnu,
        &neo,
    );
}

// ── 2. Read-a-char-mid-command ─────────────────────────────────────────

/// `zap-to-char` (`M-z x`) reads a char from the minibuffer mid-command, then
/// kills text up to and including it. From start of "foo-bar-baz", `M-z -`
/// kills "foo-" leaving "bar-baz". This reads via `read-char-from-minibuffer`
/// like the register bug — a stale-this-command-keys would self-insert the
/// target char instead of reading it.
#[test]
fn zap_to_char_reads_target_char() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "zap.txt", "foo-bar-baz\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "M-<");
    send_both(&mut gnu, &mut neo, "M-z");
    let zap_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Zap to char:"));
    gnu.read_until(Duration::from_secs(6), zap_prompt);
    neo.read_until(Duration::from_secs(8), zap_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "-");

    let zapped = |grid: &[String]| {
        grid.iter().any(|row| row.trim() == "bar-baz")
            || grid.iter().any(|row| row.contains("bar-baz"))
    };
    gnu.read_until(Duration::from_secs(6), zapped);
    neo.read_until(Duration::from_secs(8), zapped);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict("zap_to_char_reads_target_char", &gnu, &neo);
}

/// `quoted-insert` (`C-q`) reading an octal code: `C-q 1 0 1 RET` inserts the
/// character with octal code 101 = `A`. Before RET, both editors must expose
/// the same complete delayed key echo while `read-event` is still blocked;
/// after RET, both complete the command and render the same full screen.
#[test]
fn quoted_insert_octal_code_inserts_character() {
    let (mut gnu, mut neo) = boot_pair("");

    // GNU accepts any number of octal digits, so 101 remains a live sub-read
    // until a non-digit terminator arrives.
    send_both(&mut gnu, &mut neo, "C-q");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "1 0 1");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_strict("quoted_insert_octal_code_pending_subread", &gnu, &neo);

    send_both(&mut gnu, &mut neo, "RET");
    let inserted = |grid: &[String]| grid.iter().any(|row| row.trim() == "A");
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict("quoted_insert_octal_code_completed", &gnu, &neo);
}

/// `quoted-insert` of a literal control char: `C-q TAB` inserts a literal tab
/// (an actual `\t`), not indentation. `C-q` reads the next event raw.
#[test]
fn quoted_insert_literal_tab() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "x");
    send_both(&mut gnu, &mut neo, "C-q");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "TAB");
    send_both(&mut gnu, &mut neo, "y");

    // A literal tab between x and y renders as whitespace columns; both editors
    // must render the same. Just compare grids.
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    assert_pair_strict("quoted_insert_literal_tab", &gnu, &neo);
}

// ── 3. isearch ─────────────────────────────────────────────────────────

/// Incremental search (`C-s`) for a literal string, repeated with `C-s`, then
/// exited with `RET`. isearch runs its own read loop and reads keys with
/// `this-command-keys`, so it shares the command-loop sub-read machinery.
#[test]
fn isearch_forward_repeat_and_exit() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch.txt",
        "alpha one\nalpha two\nalpha three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    send_both(&mut gnu, &mut neo, "C-s");
    let prompt = |grid: &[String]| grid.iter().any(|row| row.contains("I-search:"));
    gnu.read_until(Duration::from_secs(6), prompt);
    neo.read_until(Duration::from_secs(8), prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    // Type the search term, then repeat the search (next match) with C-s.
    gnu.send(b"alpha");
    neo.send(b"alpha");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    send_both(&mut gnu, &mut neo, "C-s");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    // Exit isearch, leaving point at the second match.
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    assert_pair_strict("isearch_forward_repeat_and_exit", &gnu, &neo);
}

/// `C-g` aborts an in-progress isearch, restoring point to where the search
/// began. isearch reads keys in its own loop and uses `this-command-keys`;
/// the C-g abort must cleanly return to *scratch* state (the single-C-g and
/// this-command-keys fixes must not double-quit or leak).
#[test]
fn isearch_abort_with_keyboard_quit() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-abort.txt",
        "alpha one\nbeta two\ngamma three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    send_both(&mut gnu, &mut neo, "C-s");
    let prompt = |grid: &[String]| grid.iter().any(|row| row.contains("I-search:"));
    gnu.read_until(Duration::from_secs(6), prompt);
    neo.read_until(Duration::from_secs(8), prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    gnu.send(b"beta");
    neo.send(b"beta");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    // C-g aborts isearch and returns point to the start.
    send_both(&mut gnu, &mut neo, "C-g");
    let aborted = |grid: &[String]| {
        // Echo area no longer shows the I-search prompt.
        !grid.iter().any(|row| row.contains("I-search:"))
    };
    gnu.read_until(Duration::from_secs(6), aborted);
    neo.read_until(Duration::from_secs(8), aborted);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    // The buffer must be unchanged (no leaked "beta" self-insert), and both
    // editors must agree on the rendered grid.
    assert_pair_strict("isearch_abort_with_keyboard_quit", &gnu, &neo);
}

// ── 4. Recursive minibuffers ───────────────────────────────────────────

/// With `enable-recursive-minibuffers`, open `M-:` (eval) and *inside* it open
/// a nested `M-x` minibuffer. The nested read is exactly the save/restore path
/// bbd87e4e6 added: the inner minibuffer must save and restore the outer
/// minibuffer's `this-command-keys`, and the prompts must nest like GNU.
#[test]
fn recursive_minibuffer_eval_then_mx() {
    let (mut gnu, mut neo) = boot_pair("");

    eval_expression(&mut gnu, &mut neo, "(setq enable-recursive-minibuffers t)");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    // Open M-: (Eval:) ...
    send_both(&mut gnu, &mut neo, "M-:");
    let eval_prompt = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), eval_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    // ... then open a nested M-x inside the Eval minibuffer.
    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.last().is_some_and(|row| row.contains("M-x"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), mx_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    // Both editors should show the nested M-x minibuffer prompt.
    assert_pair_strict("recursive_minibuffer_eval_then_mx/nested", &gnu, &neo);

    // Abort the nested M-x (one C-g), then the outer Eval (one C-g): both must
    // unwind cleanly back to *scratch*.
    send_both(&mut gnu, &mut neo, "C-g");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    send_both(&mut gnu, &mut neo, "C-g");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    assert_pair_strict("recursive_minibuffer_eval_then_mx/unwound", &gnu, &neo);
}

/// Rejecting a nested `M-x` when recursive minibuffers are disabled must be
/// atomic: the outer `M-x` remains active and can still execute its command.
/// Issue #251 left Neomacs in an orphaned `*Minibuf-2*`, so the command name
/// self-inserted there instead. Inserting `X` after `forward-char` proves that
/// the outer command completed and moved point between `a` and `b`.
#[test]
fn rejected_nested_mx_keeps_outer_mx_usable() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "issue-251.txt", "ab", "C-x C-f");

    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn (setq enable-recursive-minibuffers nil) nil)"##,
    );
    let recursion_disabled = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.trim_end().ends_with("nil"))
    };
    wait_for_both(
        &mut gnu,
        &mut neo,
        Duration::from_secs(8),
        recursion_disabled,
    );
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    // Establish a strict, whole-screen baseline before the operation under
    // test. This prevents unrelated startup state from being mistaken for a
    // nested-minibuffer regression while still forbidding every grid mismatch.
    assert_grids_strict(
        "rejected nested M-x baseline",
        gnu.screen(),
        neo.screen(),
        &StrictGridOptions::default(),
    );

    send_both(&mut gnu, &mut neo, "M-x");
    let outer_mx = |grid: &[String]| grid.last().is_some_and(|row| row.contains("M-x"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), outer_mx);

    // A second M-x must report GNU's rejection without changing the selected
    // window, current buffer, minibuffer contents, or minibuffer depth.
    send_both(&mut gnu, &mut neo, "M-x");
    let rejected = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Command attempted to use minibuffer while in minibuffer"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), rejected);

    send_both_raw(&mut gnu, &mut neo, b"forward-char");
    send_both(&mut gnu, &mut neo, "RET");
    send_both_raw(&mut gnu, &mut neo, b"X");

    let recovered = |grid: &[String]| grid.iter().any(|row| row.contains("aXb"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), recovered);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    assert!(
        recovered(&gnu.text_grid()),
        "GNU outer M-x did not recover:\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        recovered(&neo.text_grid()),
        "Neomacs outer M-x did not recover:\n{}",
        neo.text_grid().join("\n")
    );
    assert_grids_strict(
        "rejected nested M-x recovery",
        gnu.screen(),
        neo.screen(),
        &StrictGridOptions::default(),
    );
}

// ── 5. Prefix args into reading commands ───────────────────────────────

/// A numeric prefix to `quoted-insert`: `C-u C-q A` inserts four `A`s (the
/// universal prefix default count is 4). The prefix arg is consumed by
/// `quoted-insert`, which then reads the char to repeat.
#[test]
fn prefix_arg_quoted_insert_repeats() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-u");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "C-q");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "A");

    let inserted = |grid: &[String]| grid.iter().any(|row| row.contains("AAAA"));
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict("prefix_arg_quoted_insert_repeats", &gnu, &neo);
}

/// A numeric prefix to the macro-replay command: define a one-char macro, then
/// replay it 5 times with `M-5 C-x e`. The prefix arg becomes the repeat
/// count passed to `call-last-kbd-macro`.
#[test]
fn prefix_arg_repeats_kbd_macro_replay() {
    let (mut gnu, mut neo) = boot_pair("");

    // Define a macro that inserts a single "z".
    send_both(&mut gnu, &mut neo, "C-x (");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "z");
    send_both(&mut gnu, &mut neo, "C-x )");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    // Replay 5 times: M-5 C-x e  -> five more "z" (total six "zzzzzz").
    send_both(&mut gnu, &mut neo, "M-5");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "C-x e");

    let replayed = |grid: &[String]| grid.iter().any(|row| row.contains("zzzzzz"));
    gnu.read_until(Duration::from_secs(6), replayed);
    neo.read_until(Duration::from_secs(8), replayed);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict("prefix_arg_repeats_kbd_macro_replay", &gnu, &neo);
}

// ── 6. C-x z repeat ────────────────────────────────────────────────────

/// `C-x z` repeats the last command, and each subsequent bare `z` repeats it
/// again. Insert "q", then `C-x z z z` repeats the self-insert. `repeat` reads
/// the repeating key via `this-command-keys` / `read-event`, so it shares the
/// sub-read machinery.
#[test]
fn repeat_command_with_cx_z() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "q");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    // C-x z repeats the self-insert of q, then bare z repeats twice more.
    send_both(&mut gnu, &mut neo, "C-x z");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "z");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "z");

    let repeated = |grid: &[String]| grid.iter().any(|row| row.contains("qqqq"));
    gnu.read_until(Duration::from_secs(6), repeated);
    neo.read_until(Duration::from_secs(8), repeated);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict("repeat_command_with_cx_z", &gnu, &neo);
}

// ── 7. Unbound key + single C-g quitting a sub-read ────────────────────

/// An unbound key in the MIDDLE of a prefix sequence: `C-x C-g`-style. Use
/// `C-x C-q` toggled then an unbound `C-x C-y`? Instead test the documented
/// undefined echo for a mid-sequence unbound key: `C-x C-_` is unbound? Use a
/// reliable unbound two-key sequence `C-c C-c` in fundamental mode? In -Q the
/// reliable unbound is `C-c c` (C-c is a prefix). Here we check the SAME echo
/// surfaces in the middle of a sequence and the editor stays responsive,
/// mirroring the event_loop guard but through the GNU oracle.
#[test]
fn unbound_key_mid_sequence_echoes_is_undefined() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-c");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "c");

    let undefined = |grid: &[String]| grid.iter().any(|row| row.contains("is undefined"));
    gnu.read_until(Duration::from_secs(6), undefined);
    neo.read_until(Duration::from_secs(8), undefined);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_strict("unbound_key_mid_sequence_echoes_is_undefined", &gnu, &neo);
}

/// A single `C-g` cleanly quits a sub-read (the `zap-to-char` minibuffer char
/// read) and returns to *scratch* state in both editors, with no double quit
/// and no leaked self-insert. Guards the single-C-g (621e5eaf0) + this-command
/// -keys (bbd87e4e6) fixes through a real sub-read of the read-char class.
#[test]
fn single_keyboard_quit_aborts_zap_char_read() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "quit-zap.txt",
        "hello world\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    send_both(&mut gnu, &mut neo, "M-z");
    let zap_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Zap to char:"));
    gnu.read_until(Duration::from_secs(6), zap_prompt);
    neo.read_until(Duration::from_secs(8), zap_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    // One C-g aborts the char read; the buffer must be unchanged ("hello world"
    // intact, no zap performed, no leaked char).
    send_both(&mut gnu, &mut neo, "C-g");
    let restored = |grid: &[String]| {
        grid.iter().any(|row| row.contains("hello world"))
            && !grid.iter().any(|row| row.contains("Zap to char:"))
    };
    gnu.read_until(Duration::from_secs(6), restored);
    neo.read_until(Duration::from_secs(8), restored);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    assert_pair_strict("single_keyboard_quit_aborts_zap_char_read", &gnu, &neo);
}
