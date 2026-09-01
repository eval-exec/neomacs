//! Event-loop / wait-machinery regression tests (Neomacs-only).
//!
//! These guard the GNU-faithful unified-wait redesign (issue #132). Each
//! test exercises one of the three things the editor blocks on through the
//! single poll primitive:
//!
//!   * **host keyboard input** — a keystroke wakes the command loop and is
//!     echoed (the cross-platform `Poller::notify` input-wakeup path);
//!   * **timer timeout** — a due timer fires while the editor sits idle in a
//!     pure-timeout wait (the wakeable poll that replaced blind
//!     `thread::sleep`);
//!   * **subprocess output** — an async child's stdout becomes readable on a
//!     poller fd, is filtered into its buffer, and drives a redisplay (this
//!     also reaps the child on EOF).
//!
//! The synchronous-shell-command test covers the fourth thing the wait loop
//! must handle: the command loop *blocking* in `wait_reading_process_output`
//! until a child exits, draining its output, then returning responsive. This
//! is also the path that issue #132 broke (a child suspending Neomacs via job
//! control). We cannot reproduce #132's suspend deterministically in a pty
//! (a synchronous shell command gives the child a pipe, not the controlling
//! terminal, and an interactive `bash -ic` would source the developer's real
//! `~/.bashrc`), so the environment-independent proof that each child runs in
//! its own process group — the actual fix — lives in the `child_isolation_tests`
//! unit test in `crates/neovm-core/src/emacs_core/system/callproc/mod.rs`.

mod support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

// ── Local helpers ───────────────────────────────────────────

/// Boot a single Neomacs TUI session and wait until *scratch* is rendered.
fn boot_neo(extra_args: &str) -> TuiSession {
    let mut neo = TuiSession::neomacs(extra_args);
    let startup_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };
    neo.read_until(Duration::from_secs(20), startup_ready);
    settle_session(&mut neo);
    neo
}

/// Eval `expression` via `M-:` and assert the echo area shows `expected`.
/// Doubles as a liveness probe: it only succeeds if the command loop is
/// actively reading and processing host input.
fn assert_eval_echoes(neo: &mut TuiSession, expression: &str, expected: &str) {
    eval_expression_one(neo, expression);
    let shows = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    neo.read_until(Duration::from_secs(8), shows);
    assert!(
        shows(&neo.text_grid()),
        "expected `{expected}` from evaluating `{expression}`:\n{}",
        neo.text_grid().join("\n")
    );
}

// ── Tests ──────────────────────────────────────────────────

/// A burst of self-inserting keystrokes must wake the command loop and
/// echo into *scratch*. This is the host-input wakeup path: the frontend
/// thread delivers the key and notifies the poller, which unblocks the
/// wait so the command loop can run.
#[test]
fn keyboard_input_echoes_into_scratch() {
    let mut neo = boot_neo("");

    neo.send(b"event-loop-typed-probe");
    let typed = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("event-loop-typed-probe"))
    };
    neo.read_until(Duration::from_secs(5), typed);

    assert!(
        typed(&neo.text_grid()),
        "self-inserting keystrokes should reach the command loop and echo \
         into *scratch*:\n{}",
        neo.text_grid().join("\n")
    );
}

/// A one-shot timer scheduled while the editor is otherwise idle must fire
/// and mutate the buffer, and the idle redisplay must paint the change
/// without any intervening keypress. Exercises the pure-timeout wakeable
/// wait that replaced blind `thread::sleep`.
#[test]
fn timer_fires_and_redisplays_while_idle() {
    let mut neo = boot_neo("");

    eval_expression_one(
        &mut neo,
        "(run-with-timer 0.2 nil (lambda () (insert \"timer-fired-probe\")))",
    );

    let fired = |grid: &[String]| grid.iter().any(|row| row.contains("timer-fired-probe"));
    neo.read_until(Duration::from_secs(5), fired);

    assert!(
        fired(&neo.text_grid()),
        "an idle one-shot timer should fire and the idle redisplay should \
         paint its buffer mutation without a keypress:\n{}",
        neo.text_grid().join("\n")
    );
}

/// An async subprocess' stdout becoming readable must drive the output
/// into *Async Shell Command* and trigger a redisplay. Exercises the
/// process-output poll backend and child reaping on EOF.
#[test]
fn async_subprocess_output_drives_redisplay() {
    let mut neo = boot_neo("");

    neo.send_key("M-&");
    let prompt_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("Async shell command:"));
    neo.read_until(Duration::from_secs(8), prompt_ready);
    neo.read(Duration::from_millis(300));

    neo.send(b"printf neo-async-probe");
    neo.send_key("RET");

    let appeared = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Async Shell Command*"))
            && grid.iter().any(|row| row.contains("neo-async-probe"))
    };
    neo.read_until(Duration::from_secs(12), appeared);

    assert!(
        appeared(&neo.text_grid()),
        "async subprocess output should appear in *Async Shell Command*:\n{}",
        neo.text_grid().join("\n")
    );
}

/// A synchronous shell command (`M-!`) blocks the command loop in
/// `wait_reading_process_output` until the child exits, draining its output.
/// The command must complete (output appears) and — critically — the editor
/// must return responsive afterward (an eval round-trips). This is the
/// command-loop side of the issue #132 fix: a child can no longer wedge or
/// suspend Neomacs while it is waited on.
#[test]
fn synchronous_shell_command_completes_and_editor_stays_responsive() {
    let mut neo = boot_neo("");

    neo.send_key("M-!");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Shell command:"));
    neo.read_until(Duration::from_secs(8), prompt_ready);
    neo.read(Duration::from_millis(300));
    neo.send(b"printf neo-sync-probe");
    neo.send_key("RET");

    let output = |grid: &[String]| grid.iter().any(|row| row.contains("neo-sync-probe"));
    neo.read_until(Duration::from_secs(8), output);
    assert!(
        output(&neo.text_grid()),
        "synchronous shell command output should appear:\n{}",
        neo.text_grid().join("\n")
    );

    // The command loop must be responsive again: an eval round-trips.
    assert_eval_echoes(&mut neo, "(+ 40 2)", "42");
}

/// Issue #132 (hang): with `shell-command-switch "-ic"`, a synchronous `M-!`
/// launches an interactive `bash -ic …`. Before the fix Neomacs wedged forever
/// in `command.output()` (`wchan = pipe_read`): the interactive child, left as
/// a background process group on Neomacs's controlling pty, was SIGTTOU/SIGTTIN-
/// stopped during its own job-control init and never exited. The fix `setsid`s
/// every pipe-stdio child (`isolate_child_command`), giving it no controlling
/// terminal, so `bash -i` degrades to "no job control" and runs to completion.
/// This guards that the synchronous shell-command returns and the editor stays
/// responsive afterward.
#[test]
fn interactive_switch_synchronous_shell_command_stays_responsive() {
    let mut neo = boot_neo("");

    eval_expression_one(&mut neo, "(setq shell-command-switch \"-ic\")");
    neo.read(Duration::from_millis(500));

    neo.send_key("M-!");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Shell command:"));
    neo.read_until(Duration::from_secs(8), prompt_ready);
    neo.read(Duration::from_millis(300));
    neo.send(b"true");
    neo.send_key("RET");
    neo.read(Duration::from_secs(2));

    // The synchronous interactive shell command must return and leave the
    // command loop responsive: an eval round-trips.
    assert_eval_echoes(&mut neo, "(+ 40 2)", "42");
}

/// C-g must interrupt a blocking `(sleep-for 20)` promptly. GNU's
/// `wait_reading_process_output` runs `maybe_quit` at the top of every
/// `while(1)` iteration when `read_kbd >= 0` (process.c:5399-5400), and
/// `Fsleep_for` passes `read_kbd = 0`, so a C-g raised while the editor sits in
/// `sleep-for` promotes to a `quit` signal within one iteration — the echo area
/// shows `Quit` and the command loop becomes responsive again. Before the fix
/// our wait loop omitted `maybe_quit`, so `(sleep-for 20)` ignored C-g for its
/// full 20 s. This guards the fix end-to-end through the real PTY/command-loop
/// stack; the deterministic unit guard is
/// `wait_until_honors_pending_quit_request_promptly` in neovm-core.
#[test]
fn keyboard_quit_interrupts_blocking_sleep_for() {
    let mut neo = boot_neo("");

    // Open the eval minibuffer and start a long blocking sleep.
    neo.send_key("M-:");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    neo.read_until(Duration::from_secs(8), prompt_ready);
    neo.read(Duration::from_millis(300));
    neo.send(b"(sleep-for 20)");
    neo.send_key("RET");

    // Let the command loop enter the blocking sleep, then fire C-g (\x07).
    neo.read(Duration::from_millis(500));
    let start = std::time::Instant::now();
    neo.send(b"\x07");

    // The quit must surface (echo area "Quit") WELL before the 20 s sleep would
    // otherwise elapse.
    let quit_shown = |grid: &[String]| grid.iter().any(|row| row.contains("Quit"));
    neo.read_until(Duration::from_secs(6), quit_shown);
    let elapsed = start.elapsed();

    assert!(
        quit_shown(&neo.text_grid()),
        "C-g should interrupt (sleep-for 20) and echo `Quit` (took {elapsed:?}):\n{}",
        neo.text_grid().join("\n")
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "C-g should interrupt sleep-for promptly, not after the full 20 s \
         (took {elapsed:?})"
    );

    // The command loop must be responsive again: an eval round-trips.
    assert_eval_echoes(&mut neo, "(+ 40 2)", "42");
}

/// The form `M-:`-evaluated by
/// [`accept_process_output_drains_while_command_input_pending`].
///
/// It reproduces the jsonrpc-request shape that hung pre-fix: a child writes a
/// full reply after a short delay, a filter inserts that reply (plus a sentinel
/// marker) into `*scratch*`, and a *synchronous* `accept-process-output` loop
/// blocks until the marker lands (with a generous self-deadline so a starved
/// neomacs blocks long past the test's read window rather than returning
/// early).  The test makes command input pending *during* this loop by typing
/// ahead, so the process fd is readable while keyboard input also waits —
/// exactly the starvation window fixed in 557701a3e.  Pre-fix neomacs
/// early-returned on the pending command input before draining the ready fd, so
/// the filter never ran and the marker never appeared.
const APO_DRAIN_FORM: &str = concat!(
    "(let ((p (make-process :name \"apo-probe\"",
    " :command (list \"sh\" \"-c\" \"sleep 0.5; printf APO-PAYLOAD\")",
    " :connection-type 'pipe",
    " :filter (lambda (_p s) (with-current-buffer \"*scratch*\"",
    " (goto-char (point-max)) (insert s)",
    " (when (string-search \"APO-PAYLOAD\" s) (insert \" APO-DRAIN-DONE\"))))))",
    " (d (+ (float-time) 20.0)))",
    " (while (and (process-live-p p)",
    " (not (with-current-buffer \"*scratch*\"",
    " (save-excursion (goto-char (point-min)) (search-forward \"APO-DRAIN-DONE\" nil t))))",
    " (< (float-time) d))",
    " (accept-process-output p 0.1)))",
);

/// Type-ahead bytes sent *while* the synchronous `accept-process-output` loop
/// is running, so command input is pending the whole time the process fd
/// becomes readable.  Plain lowercase letters self-insert harmlessly once the
/// loop returns; no RET / control chars that could disturb the minibuffer.
const APO_TYPEAHEAD: &[u8] = b"apotypeahead";

/// Interactive, GNU-vs-neomacs analogue of the deterministic unit test
/// `accept_process_output_drains_ready_output_before_yielding_to_command_input`
/// (crates/neovm-core/src/emacs_core/process_test.rs).
///
/// This is the end-to-end shape that would have CAUGHT the hang fixed in
/// 557701a3e ("fix(wait): drain ready process output before yielding to command
/// input").  A timer/jsonrpc callback re-entering `accept-process-output` (e.g.
/// Copilot startup) hung forever because pending command input made the wait
/// loop early-return *before* draining a process fd that already had a full
/// reply waiting.
///
/// Reproduction:
///   1. `M-:` evaluate [`APO_DRAIN_FORM`] in both engines — it starts a child
///      that replies after a 0.5 s delay and *synchronously* waits for it.
///   2. Immediately type ahead ([`APO_TYPEAHEAD`]) so command input is pending
///      while the wait loop runs; the process fd becomes readable (after the
///      delay) *while* keyboard input also waits — the starvation window.
///   3. Assert the sentinel `APO-DRAIN-DONE` (inserted only after the payload
///      is drained by the filter) appears on BOTH engines.
///
/// GNU and post-fix neomacs drain the fd and show the marker promptly; pre-fix
/// neomacs starves it (marker never appears within the read window → the form
/// blocks on its own 20 s deadline → timeout).  The deterministic guard for the
/// exact ordering remains the unit test cited above; this is the interactive
/// proof that the behavior holds through the real PTY/command-loop stack.
#[test]
fn accept_process_output_drains_while_command_input_pending() {
    let (mut gnu, mut neo) = boot_pair("");

    // Open the eval minibuffer and send the synchronous-wait form.
    send_both(&mut gnu, &mut neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    gnu.send(APO_DRAIN_FORM.as_bytes());
    neo.send(APO_DRAIN_FORM.as_bytes());
    // RET submits the eval; the synchronous wait loop starts running now.
    send_both(&mut gnu, &mut neo, "RET");

    // Let RET be consumed and the wait loop start spinning while the child is
    // still in its 0.5 s sleep (its output is NOT yet readable).  Then type
    // ahead so command input is pending *before* the process fd becomes
    // readable — and stays pending for the whole remaining loop.  When the
    // child finally writes, the fd is readable while keyboard input also waits:
    // the exact starvation window.  Pre-fix neomacs early-returns on the
    // pending input and never drains the fd.
    read_both(&mut gnu, &mut neo, Duration::from_millis(200));
    gnu.send(APO_TYPEAHEAD);
    neo.send(APO_TYPEAHEAD);

    // The marker is inserted only after the payload is drained by the filter.
    let drained = |grid: &[String]| grid.iter().any(|row| row.contains("APO-DRAIN-DONE"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), drained);

    assert!(
        drained(&gnu.text_grid()),
        "GNU: process output must drain while command input is pending \
         (APO-DRAIN-DONE should appear):\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        drained(&neo.text_grid()),
        "Neomacs: process output must drain while command input is pending — \
         pre-fix this starved (early-return on pending command input before \
         draining the ready process fd) so APO-DRAIN-DONE never appeared:\n{}",
        neo.text_grid().join("\n")
    );
}

/// Finding 1 (keyboard/command-loop audit) — pressing a truly-unbound key
/// must give the GNU feedback: the echo area shows "<key> is undefined"
/// (the `undefined` command in subr.el). Before the fix the command loop
/// short-circuited the nil-binding case with a bare `continue`, so the key
/// was silent and ran no per-command hooks.
///
/// `C-c c` is a reliable unbound sequence in `-Q` (`C-c` is a prefix and
/// `c` after it is unbound in the default *scratch* mode).
#[test]
fn unbound_key_echoes_is_undefined() {
    let mut neo = boot_neo("");

    neo.send_key("C-c");
    neo.read(Duration::from_millis(300));
    neo.send_key("c");

    let undefined = |grid: &[String]| grid.iter().any(|row| row.contains("is undefined"));
    neo.read_until(Duration::from_secs(6), undefined);
    assert!(
        undefined(&neo.text_grid()),
        "an unbound key (C-c c) should echo \"... is undefined\":\n{}",
        neo.text_grid().join("\n")
    );

    // The command loop must remain responsive after an unbound key.
    assert_eval_echoes(&mut neo, "(+ 40 2)", "42");
}

/// Finding 3 (keyboard/command-loop audit) — a single idle C-g at top
/// level must run `keyboard-quit` exactly once (echo area "Quit"), not a
/// double quit, and the editor must stay responsive. Guards that the
/// cross-thread `quit_requested` atomic the input bridge raises for a C-g
/// is fully accounted for by the one `keyboard-quit` and does not leak a
/// second, spurious quit.
#[test]
fn single_keyboard_quit_echoes_quit_once_and_stays_responsive() {
    let mut neo = boot_neo("");

    let start = std::time::Instant::now();
    neo.send(b"\x07"); // a single C-g

    let quit_shown = |grid: &[String]| grid.iter().any(|row| row.contains("Quit"));
    neo.read_until(Duration::from_secs(6), quit_shown);
    assert!(
        quit_shown(&neo.text_grid()),
        "a single C-g should echo `Quit' (took {:?}):\n{}",
        start.elapsed(),
        neo.text_grid().join("\n")
    );

    // Responsiveness probe: an eval round-trips, proving the command loop
    // was not wedged or left in a recurring-quit state by the single C-g.
    assert_eval_echoes(&mut neo, "(+ 40 2)", "42");
}
