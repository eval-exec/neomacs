#![allow(dead_code)]

use crate::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Maximum total time for GNU Emacs to reach the startup predicate.
const GNU_STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
/// Maximum total time for Neomacs to reach the startup predicate.
const NEO_STARTUP_TIMEOUT: Duration = Duration::from_secs(18);
/// Idle-settle timeout used after startup — keep reading until the
/// grid is stable for this long with no round cap.
const SETTLE_IDLE: Duration = Duration::from_millis(500);
/// Granularity for interleaved poll of both PTYs during parallel boot.
const POLL_SLICE: Duration = Duration::from_millis(80);

// ── boot_pair (canonical) ──────────────────────────────────────────────

/// Boot GNU Emacs and Neomacs side-by-side.
///
/// # Phases
///
/// 1. **Concurrent poll** — both processes are spawned and their PTYs are
///    drained in interleaved short slices. As soon as one editor reaches the
///    startup predicate it stops polling that PTY, so the faster editor
///    never waits for the slower one.
///
/// 2. **Uncapped settle** — after the predicate fires, the session is
///    read in rounds until its rendered grid stops changing. No round
///    limit — it keeps going until the grid is truly stable. This
///    absorbs late-startup display bursts without a blind `sleep`.
pub fn boot_pair(extra_args: &str) -> (TuiSession, TuiSession) {
    boot_pair_with_erase_char(extra_args, PtyEraseChar::TerminalDefault)
}

/// Boot both editors on PTYs whose ERASE character is ERASE.
///
/// The erase byte decides whether `normal-erase-is-backspace-mode` turns on,
/// so this is how a case reaches the `^H`-terminal behaviour the pty default
/// never exercises.
pub fn boot_pair_with_erase_char(
    extra_args: &str,
    erase: PtyEraseChar,
) -> (TuiSession, TuiSession) {
    let mut gnu = TuiSession::gnu_emacs_with_erase_char(extra_args, erase);
    let mut neo = TuiSession::neomacs_with_erase_char(extra_args, erase);

    let startup_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };

    // Phase 1 — interleaved concurrent poll
    let gnu_deadline = Instant::now() + GNU_STARTUP_TIMEOUT;
    let neo_deadline = Instant::now() + NEO_STARTUP_TIMEOUT;
    let mut gnu_ready = false;
    let mut neo_ready = false;

    while !gnu_ready || !neo_ready {
        let now = Instant::now();
        if now >= gnu_deadline && now >= neo_deadline {
            break;
        }
        if !gnu_ready && now < gnu_deadline {
            let cap = gnu_deadline.saturating_duration_since(now).min(POLL_SLICE);
            gnu.read(cap);
            gnu_ready = startup_ready(&gnu.text_grid());
        }
        if !neo_ready && now < neo_deadline {
            let cap = neo_deadline.saturating_duration_since(now).min(POLL_SLICE);
            neo.read(cap);
            neo_ready = startup_ready(&neo.text_grid());
        }
    }
    // Phase 2 — parallel settle (absorbs late-render bursts)
    settle_both(&mut gnu, &mut neo);

    (gnu, neo)
}

/// Read `session` until its rendered grid stops changing, with no round cap.
pub fn settle_session(session: &mut TuiSession) {
    let mut previous = session.text_grid();
    loop {
        session.read(SETTLE_IDLE);
        let current = session.text_grid();
        if current == previous {
            return;
        }
        previous = current;
    }
}

/// Parallel settle via `read_both`: keep reading both sessions until both
/// grids stop changing. Used by `boot_pair` so the slower editor doesn't
/// stretch the settle phase.
fn settle_both(gnu: &mut TuiSession, neo: &mut TuiSession) {
    let mut prev_gnu = gnu.text_grid();
    let mut prev_neo = neo.text_grid();
    loop {
        read_both(gnu, neo, SETTLE_IDLE);
        let cur_gnu = gnu.text_grid();
        let cur_neo = neo.text_grid();
        if cur_gnu == prev_gnu && cur_neo == prev_neo {
            return;
        }
        prev_gnu = cur_gnu;
        prev_neo = cur_neo;
    }
}

// ── Shared helpers ─────────────────────────────────────────────────────

/// Send the same key sequence to both sessions, interleaving each key so
/// both editors receive it at roughly the same time. Only one 50 ms delay
/// per key instead of two (one per session).
pub fn send_both(gnu: &mut TuiSession, neo: &mut TuiSession, keys: &str) {
    for part in keys.split_whitespace() {
        let bytes = emacs_key(part);
        gnu.send(&bytes);
        neo.send(&bytes);
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn send_both_raw(gnu: &mut TuiSession, neo: &mut TuiSession, bytes: &[u8]) {
    gnu.send(bytes);
    neo.send(bytes);
}

/// Drain PTY output from both sessions in parallel via scoped threads.
/// Each session gets the full timeout with its own idle detection — the
/// faster editor returns as soon as its output settles, never waiting
/// for the slower one.
pub fn read_both(gnu: &mut TuiSession, neo: &mut TuiSession, timeout: Duration) {
    std::thread::scope(|s| {
        s.spawn(|| gnu.read(timeout));
        s.spawn(|| neo.read(timeout));
    });
}

pub fn resize_both(gnu: &mut TuiSession, neo: &mut TuiSession, rows: u16, cols: u16) {
    gnu.resize(rows, cols);
    neo.resize(rows, cols);
}

/// Wait until `predicate` is satisfied on both sessions or `timeout`
/// elapses. Polls both PTYs concurrently in short interleaved slices,
/// same strategy as `boot_pair` phase 1.
pub fn wait_for_both<F>(gnu: &mut TuiSession, neo: &mut TuiSession, timeout: Duration, predicate: F)
where
    F: Fn(&[String]) -> bool + Copy,
{
    let deadline = Instant::now() + timeout;
    let mut gnu_ok = predicate(&gnu.text_grid());
    let mut neo_ok = predicate(&neo.text_grid());
    while !gnu_ok || !neo_ok {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let cap = deadline.saturating_duration_since(now).min(POLL_SLICE);
        if !gnu_ok {
            gnu.read(cap);
            gnu_ok = predicate(&gnu.text_grid());
        }
        if !neo_ok {
            neo.read(cap);
            neo_ok = predicate(&neo.text_grid());
        }
    }
}

// ── Higher-level workflow helpers ──────────────────────────────────────

pub fn invoke_mx_command(gnu: &mut TuiSession, neo: &mut TuiSession, command: &str) {
    send_both(gnu, neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.last().is_some_and(|row| row.contains("M-x"));
    wait_for_both(gnu, neo, Duration::from_secs(8), mx_prompt);
    read_both(gnu, neo, Duration::from_millis(300));

    gnu.send(command.as_bytes());
    neo.send(command.as_bytes());
    send_both(gnu, neo, "RET");
}

/// Wait for GNU's asynchronous `execute-extended-command` binding suggestion.
///
/// `execute-extended-command` schedules this message with `run-at-time`, after
/// the invoked command returns and `real-last-command` is committed.  A screen
/// comparison that expects the suggestion must therefore synchronize on that
/// observable state rather than merely on the command's primary effect.
pub fn wait_for_both_mx_suggestion(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    command: &str,
    timeout: Duration,
) {
    let suggestion_ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("You can run the command")
                && row.contains(command)
                && row.contains(" with ")
        })
    };
    wait_for_both(gnu, neo, timeout, suggestion_ready);

    for (label, session) in [("GNU", gnu), ("Neomacs", neo)] {
        assert!(
            suggestion_ready(&session.text_grid()),
            "{label} did not display the M-x suggestion for {command:?}:\n{}",
            session.text_grid().join("\n")
        );
    }
}

pub fn eval_expression(gnu: &mut TuiSession, neo: &mut TuiSession, expression: &str) {
    send_both(gnu, neo, "M-:");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    wait_for_both(gnu, neo, Duration::from_secs(8), prompt_ready);
    read_both(gnu, neo, Duration::from_millis(300));
    gnu.paste(expression);
    neo.paste(expression);
    send_both(gnu, neo, "RET");
}

pub fn eval_expression_one(session: &mut TuiSession, expression: &str) {
    session.send_key("M-:");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    session.read_until(Duration::from_secs(8), prompt_ready);
    session.read(Duration::from_millis(300));
    session.paste(expression);
    session.send_key("RET");
}

/// Remove repository-specific state from paired source-tree fixtures by
/// disabling VC entirely for the rest of the workflow.
///
/// The paired executables necessarily visit different copies of these
/// fixtures: Neomacs opens its source-tree file inside the CI checkout,
/// while the installed GNU oracle's copy lives outside any Git repository.
/// With VC enabled GNU's mode line then carries no backend segment while
/// Neomacs shows `Git', so the paired displays cannot agree off a machine
/// whose GNU also runs from a Git checkout.  `vc-handled-backends nil'
/// drops the segment on both sides; the rest of the suite keeps its
/// explicit VC/modeline coverage.
pub fn disable_vc_backends(gnu: &mut TuiSession, neo: &mut TuiSession) {
    eval_expression(gnu, neo, "(setq vc-handled-backends nil)");
}

/// Keep VC enabled while removing repository-specific state from paired
/// source-tree fixtures.
///
/// GNU `vc-mode-line` displays only the backend name when
/// `vc-display-status` is nil.  This preserves coverage that the visited file
/// is recognized as Git-controlled, while making the exact display independent
/// of the two executables' necessarily different checkout branch/revision.
/// Requires both sides' fixtures to live inside Git checkouts; the installed
/// CI oracle's files do not, so `mx_view_hello_file_strict_match` cannot use
/// this there.
pub fn use_backend_only_vc_mode_line(gnu: &mut TuiSession, neo: &mut TuiSession) {
    eval_expression(gnu, neo, "(setq vc-display-status nil)");
}

/// Make GNU `compilation-handle-exit`'s two wall-clock fields deterministic.
///
/// The advice is scoped to that function: editor timers and process polling
/// continue to observe real time, while the buffer annotation always receives
/// the same timestamp and a 0.01-second elapsed duration in both sessions.
pub fn use_deterministic_compilation_exit_time(gnu: &mut TuiSession, neo: &mut TuiSession) {
    let expression = r##"(progn
      (require 'compile)
      (require 'cl-lib)
      (defun neomacs-tui--stable-compilation-exit-time (original &rest args)
        (cl-letf (((symbol-function 'current-time-string)
                   (lambda (&optional _time _zone) "Wed Sep  2 00:00:00 2026"))
                  ((symbol-function 'float-time)
                   (lambda (&optional _time) (+ compilation--start-time 0.01))))
          (apply original args)))
      (advice-add 'compilation-handle-exit :around
                  #'neomacs-tui--stable-compilation-exit-time))"##;
    eval_expression(gnu, neo, expression);
}

/// Keep About screens independent of the two binaries' build directories,
/// feature sets, and dump dates while preserving `emacs-version`'s public
/// interactive/return/insert behavior.
///
/// The source version and target configuration remain observable and must
/// still agree. Only GNU `version.el`'s explicitly environmental fields are
/// fixed: build number, window-system feature suffixes, and build time.
pub fn use_deterministic_emacs_version(gnu: &mut TuiSession, neo: &mut TuiSession) {
    let expression = r##"(progn
      (defun neomacs-tui--stable-emacs-version (&optional here)
        (interactive "P")
        (let ((version-string
               (format "GNU Emacs %s (build 1, %s)%s of 2000-01-01"
                       emacs-version
                       system-configuration
                       (if (called-interactively-p 'interactive) "" "\n"))))
          (if here
              (insert version-string)
            (if (called-interactively-p 'interactive)
                (message "%s" version-string)
              version-string))))
      (advice-add 'emacs-version :override
                  #'neomacs-tui--stable-emacs-version))"##;
    eval_expression(gnu, neo, expression);
}

/// Keep Dired's free-space annotation enabled while fixing its OS observation.
///
/// GNU's own Dired tests stub `file-system-info`: free bytes can change between
/// two editor processes even when both display the same test-owned directory.
pub fn use_deterministic_file_system_info(gnu: &mut TuiSession, neo: &mut TuiSession) {
    let expression = r##"(progn
      (defun neomacs-tui--stable-file-system-info (_original _filename)
        '(107374182400 53687091200 1073741824))
      (advice-add 'file-system-info :around
                  #'neomacs-tui--stable-file-system-info))"##;
    eval_expression(gnu, neo, expression);
}

pub fn open_home_file(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    name: &str,
    contents: &str,
    keys: &str,
) {
    write_home_file(gnu, name, contents);
    write_home_file(neo, name, contents);

    send_both(gnu, neo, keys);
    let minibuffer_path = format!("~/{name}");
    gnu.send(minibuffer_path.as_bytes());
    neo.send(minibuffer_path.as_bytes());
    send_both(gnu, neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains(name))
            && grid.iter().any(|row| {
                contents
                    .lines()
                    .next()
                    .is_some_and(|line| row.contains(line))
            })
    };
    wait_for_both(gnu, neo, Duration::from_secs(20), ready);
    read_both(gnu, neo, Duration::from_secs(1));
}

pub fn open_file_path(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    path: &Path,
    first_line: &str,
    keys: &str,
) {
    send_both(gnu, neo, keys);
    let path_str = path.to_string_lossy();
    gnu.send(path_str.as_bytes());
    neo.send(path_str.as_bytes());
    send_both(gnu, neo, "RET");

    let file_name = Path::new(path_str.as_ref())
        .file_name()
        .and_then(|name| name.to_str())
        .expect("test path should have a utf-8 file name")
        .to_string();
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains(&file_name))
            && grid.iter().any(|row| row.contains(first_line))
    };
    wait_for_both(gnu, neo, Duration::from_secs(20), ready);
    read_both(gnu, neo, Duration::from_secs(1));
}

pub fn save_current_file_and_assert_contents(
    label: &str,
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    name: &str,
    expected: &str,
) {
    send_both(gnu, neo, "C-x C-s");

    let gnu_path = gnu.home_dir().join(name);
    let neo_path = neo.home_dir().join(name);
    for _ in 0..10 {
        read_both(gnu, neo, Duration::from_millis(300));
        let gnu_saved = fs::read_to_string(&gnu_path).ok().as_deref() == Some(expected);
        let neo_saved = fs::read_to_string(&neo_path).ok().as_deref() == Some(expected);
        if gnu_saved && neo_saved {
            break;
        }
    }

    assert_eq!(
        fs::read_to_string(&gnu_path).expect("read GNU saved file"),
        expected,
        "{label}: GNU saved file contents should match"
    );
    assert_eq!(
        fs::read_to_string(&neo_path).expect("read Neo saved file"),
        expected,
        "{label}: Neomacs saved file contents should match"
    );
}

// ── Assertion and utility helpers ──────────────────────────────────────

/// Assert complete GNU/Neomacs display parity.
pub fn assert_pair_exact_display(label: &str, gnu: &TuiSession, neo: &TuiSession) {
    let report = compare_session_displays(gnu, neo);
    assert_pair_display_report(label, gnu, neo, report);
}

/// Assert complete display parity while declaring additional pairs of
/// concrete spellings for the same test-owned resources.
///
/// Each pair is one GNU spelling and its Neomacs counterpart; the
/// comparator rewrites both to a shared placeholder before comparing, so
/// resources the two executables necessarily name differently — session
/// homes, generated temporaries, or fragments of them left over by
/// terminal line wrapping — do not register as display differences.
pub fn assert_pair_exact_display_with_path_pairs(
    label: &str,
    gnu: &TuiSession,
    neo: &TuiSession,
    pairs: &[(String, String)],
) {
    let mut environment = PairedDisplayEnvironment::from_sessions(gnu, neo);
    for (gnu_spelling, neomacs_spelling) in pairs {
        environment = environment.with_path_pair(gnu_spelling, neomacs_spelling);
    }
    let report = compare_displays_in_environment(gnu.screen(), neo.screen(), &environment);
    assert_pair_display_report(label, gnu, neo, report);
}

/// Assert complete display parity while declaring one additional pair of
/// concrete path spellings for the same test-owned resource.
pub fn assert_pair_exact_display_with_path_pair(
    label: &str,
    gnu: &TuiSession,
    neo: &TuiSession,
    gnu_path: &str,
    neo_path: &str,
) {
    let environment =
        PairedDisplayEnvironment::from_sessions(gnu, neo).with_path_pair(gnu_path, neo_path);
    let report = compare_displays_in_environment(gnu.screen(), neo.screen(), &environment);
    assert_pair_display_report(label, gnu, neo, report);
}

fn assert_pair_display_report(
    label: &str,
    gnu: &TuiSession,
    neo: &TuiSession,
    report: DisplayReport,
) {
    if report.is_satisfied() {
        return;
    }

    for difference in report.unexpected().iter().take(40) {
        eprintln!("  unexpected display difference: {difference:#?}");
    }
    if let Some(DisplayDifference::StyleClass { cell, .. }) = report
        .unexpected()
        .iter()
        .find(|difference| matches!(difference, DisplayDifference::StyleClass { .. }))
    {
        for (editor, screen) in [("GNU", gnu.screen()), ("Neomacs", neo.screen())] {
            let raw = screen
                .cell(cell.row, cell.column)
                .expect("reported style cell is inside terminal geometry");
            eprintln!(
                "  {editor} style cell {cell:?}: contents={:?} fg={:?} bg={:?} bold={} dim={} italic={} underline={} inverse={}",
                raw.contents(),
                raw.fgcolor(),
                raw.bgcolor(),
                raw.bold(),
                raw.dim(),
                raw.italic(),
                raw.underline(),
                raw.inverse(),
            );
        }
    }
    if report.unexpected().len() > 40 {
        eprintln!(
            "  ... and {} more unexpected display differences",
            report.unexpected().len() - 40
        );
    }
    panic!("{label} violated exact TUI display parity");
}

/// Predicate: the `*scratch*` buffer is visible with its default content.
pub fn scratch_ready(grid: &[String]) -> bool {
    grid.iter().any(|row| row.contains("*scratch*"))
        && grid
            .iter()
            .any(|row| row.contains("This buffer is for text that is not saved"))
}

/// Dump both editor grids and their diffs to stderr for debugging.
pub fn dump_pair_grids(label: &str, gnu: &TuiSession, neo: &TuiSession) {
    eprintln!("{label}: GNU grid");
    for (row, text) in gnu.text_grid().iter().enumerate() {
        eprintln!("  {row:02}: |{}|", text.trim_end());
    }
    eprintln!("{label}: NEO grid");
    for (row, text) in neo.text_grid().iter().enumerate() {
        eprintln!("  {row:02}: |{}|", text.trim_end());
    }
    let report = compare_session_displays(gnu, neo);
    if !report.is_satisfied() {
        eprintln!(
            "{label}: {} exact display differences",
            report.unexpected().len()
        );
        for difference in report.unexpected().iter().take(40) {
            eprintln!("  {difference:#?}");
        }
    }
}

/// Send `C-h` then a help sub-key, waiting for the prefix to appear.
pub fn send_help_sequence(gnu: &mut TuiSession, neo: &mut TuiSession, key: &str) {
    send_both(gnu, neo, "C-h");
    let prefix_ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("C-h-")
                || row.contains("C-h (Type ? for further options")
                || row.contains("C-h (Type C-h for more help")
        })
    };
    gnu.read_until(Duration::from_secs(6), prefix_ready);
    neo.read_until(Duration::from_secs(8), prefix_ready);
    read_both(gnu, neo, Duration::from_millis(300));
    send_both(gnu, neo, key);
}

/// Send `C-g` (keyboard-quit) and wait until `*scratch*` is visible again.
pub fn abort_minibuffer_and_wait_for_scratch(gnu: &mut TuiSession, neo: &mut TuiSession) {
    send_both(gnu, neo, "C-g");
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(gnu, neo, Duration::from_secs(1));
}

/// Assert that a file in each editor's home dir matches the expected contents.
pub fn assert_home_file_contents(gnu: &TuiSession, neo: &TuiSession, name: &str, expected: &str) {
    assert_eq!(
        fs::read_to_string(gnu.home_dir().join(name)).expect("read GNU home file"),
        expected,
        "GNU file contents should match"
    );
    assert_eq!(
        fs::read_to_string(neo.home_dir().join(name)).expect("read Neo home file"),
        expected,
        "Neomacs file contents should match"
    );
}

// ── Event-ordered barriers ─────────────────────────────────────────────

/// A per-command record written by the editor's own `post-command-hook`.
///
/// Some waits cannot be phrased as a screen predicate because the awaited
/// event's visible effect is a no-op — `DEL` on an empty minibuffer prompt
/// must be *processed* while leaving the screen unchanged, so "still looks
/// right" is true both before and after. Waiting a duration guesses; this
/// barrier observes the editor's command loop directly: every executed
/// command appends its name to a per-session trace file, so waiting for the
/// line count to advance by N is an ordered event, independent of machine
/// speed.
///
/// Install before the interaction under test (the install itself is input,
/// so it cannot run once a minibuffer is open), take a baseline with
/// [`CommandTrace::mark`], then wait for the expected number of commands.
/// Follow with a drain (the suite's fixed `read_both` tails do this):
/// `post-command-hook` runs before redisplay, so the trace proves input was
/// consumed while the drain lets the resulting frame be painted.
pub struct CommandTrace {
    gnu_path: PathBuf,
    neo_path: PathBuf,
}

impl CommandTrace {
    /// Install the trace hook in both editors and return their log paths.
    pub fn install(gnu: &mut TuiSession, neo: &mut TuiSession) -> Self {
        let gnu_path = gnu.temp_dir().join("command-trace");
        let neo_path = neo.temp_dir().join("command-trace");
        for path in [&gnu_path, &neo_path] {
            let _ = fs::remove_file(path);
        }
        let install = |path: &Path| {
            format!(
                r#"(progn
                     (defun neomacs-tui--trace-command ()
                       (ignore-errors
                         (let ((inhibit-message t))
                           (write-region (format "%S\n" this-command) nil {path:?} 'append))))
                     (add-hook 'post-command-hook #'neomacs-tui--trace-command))"#,
            )
        };
        eval_expression_one(gnu, &install(&gnu_path));
        eval_expression_one(neo, &install(&neo_path));
        Self { gnu_path, neo_path }
    }

    /// Count commands executed so far, as `(gnu, neomacs)`.
    pub fn mark(&self) -> (usize, usize) {
        (trace_lines(&self.gnu_path), trace_lines(&self.neo_path))
    }

    /// Block until both editors have executed `delta` more commands than
    /// `baseline`, re-reading their PTYs while waiting.
    pub fn wait_for(
        &self,
        gnu: &mut TuiSession,
        neo: &mut TuiSession,
        baseline: (usize, usize),
        delta: usize,
        timeout: Duration,
    ) {
        let deadline = Instant::now() + timeout;
        loop {
            let now = self.mark();
            if now.0 >= baseline.0 + delta && now.1 >= baseline.1 + delta {
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            read_both(gnu, neo, remaining.min(Duration::from_millis(100)));
        }
        panic!(
            "command trace did not advance: baseline {baseline:?} want +{delta}, got {:?}\n\
             GNU trace:\n{}\nNeomacs trace:\n{}",
            self.mark(),
            fs::read_to_string(&self.gnu_path).unwrap_or_default(),
            fs::read_to_string(&self.neo_path).unwrap_or_default(),
        );
    }
}

fn trace_lines(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|contents| contents.lines().filter(|line| !line.is_empty()).count())
        .unwrap_or(0)
}

// ── File helpers ──────────────────────────────────────────────────────

pub fn write_home_file(session: &TuiSession, name: &str, contents: &str) {
    let path = session.home_dir().join(name);
    fs::write(path, contents).expect("write test file in isolated HOME");
}

/// Boot a pair that will EDIT a file both sessions have open.
///
/// [`write_shared_temp_file`] deliberately points both editors at the SAME
/// path so their diff headers and echoed paths match on screen. Real Emacs
/// correctly objects to that: whichever editor modifies the buffer first
/// writes a .#lock beside the file, and the second one stops at
/// ask-user-about-lock ("locked by user@host (pid N)") instead of editing.
///
/// Which editor gets there first is a RACE, so a shared-file editing test is
/// only coherent with locking off. It passed for a long time on the ordering
/// happening to favour GNU, and any change to how fast either side lays out a
/// row can flip it. The test is about diff-buffer-with-file, not about lock
/// arbitration between two editors that are really the same user.
pub fn boot_pair_editing_a_shared_file() -> (TuiSession, TuiSession) {
    boot_pair("--eval=(set-default'create-lockfiles())")
}

/// Write a file to a shared temp location and return its absolute path.
/// Both GNU and Neo can open this same path, so diff headers etc. match.
/// Uses a short directory name to avoid line-wrapping differences.
pub fn write_shared_temp_file(name: &str, contents: &str) -> TuiTempFile {
    TuiTempFile::new("neomacs-tui-shared-", name, contents)
}

/// Open a file at an absolute path in both sessions.
pub fn open_shared_file(gnu: &mut TuiSession, neo: &mut TuiSession, path: &Path, keys: &str) {
    send_both(gnu, neo, keys);
    let path_str = path.to_string_lossy();
    gnu.send(path_str.as_bytes());
    neo.send(path_str.as_bytes());
    send_both(gnu, neo, "RET");

    let name = path.file_name().unwrap().to_string_lossy();
    let contents = fs::read_to_string(path).expect("read shared test file");
    let first_line = contents.lines().next().unwrap_or("");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains(name.as_ref()))
            && grid.iter().any(|row| row.contains(first_line))
    };
    wait_for_both(gnu, neo, Duration::from_secs(20), ready);
    read_both(gnu, neo, Duration::from_secs(1));
}
