#![cfg(unix)]
//! Startup on a terminal that auto-enables xterm mouse tracking still runs `-l`.
//!
//! `xterm--init` (lisp/term/xterm.el:1035-1044) calls `(xterm-mouse-mode 1)`
//! whenever TERM matches `xterm--auto-xt-mouse-allowed-types`
//! (lisp/term/xterm.el:134-140) -- `alacritty` and `contour` -- and TERM=alacritty
//! reaches `terminal-init-xterm` at all because `term-file-aliases`
//! (lisp/faces.el:38-51) maps it to "xterm".  Enabling the mode maps
//! `turn-on-xterm-mouse-tracking-on-terminal` over `(terminal-list)`
//! (lisp/xt-mouse.el:413), and that function asks `(frame-initial-p TERMINAL)`
//! (lisp/xt-mouse.el:512) -- a TERMINAL, not a frame, which GNU's
//! `Fframe_initial_p` (src/terminal.c:482-500) answers by design.
//!
//! All of that happens inside `tty-run-terminal-initialization`, i.e. BEFORE
//! `command-line-1` processes the command line.  A signal there does not merely
//! print a message: it aborts startup argument handling, so `-l FILE` and
//! `--eval FORM` never run.  That is what this suite gates -- the swallowed
//! `-l`, not the message -- against GNU on the same pty and the same TERM.
//!
//! Ledger 160.  `xterm-256color` is the control: it matches neither allowlist
//! without an XTVERSION reply, so the auto-enable never fires there and the same
//! startup succeeded even while alacritty failed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

/// TERM values, and whether `xterm--init` auto-enables `xterm-mouse-mode` on them.
const TERMINALS: [(&str, bool); 2] = [("alacritty", true), ("xterm-256color", false)];

/// Facts each editor records from inside a fully initialized tty session.
///
/// The first line is the one that matters most: it only ever gets written if
/// `-l` ran at all.  The rest describe the mouse-tracking setup that the
/// `frame-initial-p` answer gates, so a session that starts but silently skips
/// tracking is not mistaken for parity.
const PROBE_EL: &str = r##"
(with-temp-file (getenv "NEOMACS_STARTUP_PROBE_OUT")
  (insert (format "dash-l ran\n"))
  (insert (format "tty-type %s\n" (tty-type (selected-frame))))
  (insert (format "terminal-initted %s\n"
                  (terminal-parameter nil 'terminal-initted)))
  (insert (format "terminal-live-p %S\n" (terminal-live-p (frame-terminal))))
  (insert (format "frame-initial-p/frame %S\n" (frame-initial-p (selected-frame))))
  (insert (format "frame-initial-p/terminal %S\n" (frame-initial-p (frame-terminal))))
  (insert (format "xterm-mouse-mode %S\n" (and (boundp 'xterm-mouse-mode)
                                               (symbol-value 'xterm-mouse-mode))))
  (insert (format "tracking %S\n" (and (terminal-parameter nil 'xterm-mouse-mode) t))))
(kill-emacs 0)
"##;

struct Editor {
    name: &'static str,
    program: PathBuf,
    extra_args: Vec<String>,
}

fn gnu() -> Editor {
    Editor {
        name: "GNU",
        program: PathBuf::from("emacs"),
        extra_args: vec![
            "-no-comp-spawn".to_string(),
            "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))".to_string(),
        ],
    }
}

fn neomacs() -> Editor {
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "neomacs binary not found at {}",
        program.display()
    );
    Editor {
        name: "Neomacs",
        program,
        extra_args: Vec::new(),
    }
}

/// What one editor did on one TERM.
struct Startup {
    /// Did the editor exit on its own, i.e. did `-l`'s `kill-emacs` run?
    exited: bool,
    /// What `-l` recorded, or `None` when `-l` never ran.
    facts: Option<String>,
}

/// Run `EDITOR -nw -Q -l PROBE` on a real pty of `term` and collect what it did.
///
/// A startup that swallows `-l` does not fail loudly -- it drops into the
/// interactive command loop and waits forever -- so the deadline is part of the
/// measurement rather than a panic: `exited == false` IS the failure mode under
/// test, and the assertion that names it belongs in the test.
fn run_startup(editor: &Editor, term: &str, budget: Duration) -> Startup {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-startup-probe-");
    let script = home.path().join("probe.el");
    std::fs::write(&script, PROBE_EL).expect("write probe elisp");
    let facts_path = home.path().join("facts.txt");

    let mut launch = TuiLaunch::new(editor.program.as_os_str())
        .arg("-nw")
        .arg("-Q");
    for arg in &editor.extra_args {
        launch = launch.arg(arg);
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("NEOMACS_STARTUP_PROBE_OUT", &facts_path)
        .env_remove("COLORTERM");
    let session_name = format!("{}-{term}", editor.name);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        &session_name,
        TuiTerminalConfig::new(term, 24, 80),
    );
    let exited = session.run_to_completion(budget) == TuiProcessOutcome::Exited;

    Startup {
        exited,
        facts: read_facts(&facts_path),
    }
}

fn read_facts(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().filter(|s| !s.is_empty())
}

/// The gate: on every TERM here, `-l` must run, and it must record what GNU
/// records.
#[test]
fn startup_runs_dash_l_on_every_terminal_gnu_does() {
    let gnu_editor = gnu();
    let neomacs_editor = neomacs();
    let budget = Duration::from_secs(120);
    let mut report = Vec::new();

    for (term, auto_mouse) in TERMINALS {
        let gnu_run = run_startup(&gnu_editor, term, budget);
        let gnu_facts = gnu_run.facts.unwrap_or_else(|| {
            panic!("GNU itself never ran -l on TERM={term}; the harness is wrong, not neomacs")
        });
        assert!(gnu_run.exited, "GNU did not exit on TERM={term}");

        let run = run_startup(&neomacs_editor, term, budget);
        let facts = run.facts.unwrap_or_else(|| {
            panic!(
                "neomacs swallowed -l on TERM={term} (auto xterm-mouse: {auto_mouse}); \
                 startup aborted before command-line-1 processed the arguments. GNU recorded:\n{gnu_facts}"
            )
        });
        assert!(
            run.exited,
            "neomacs ran -l on TERM={term} but never exited; GNU recorded:\n{gnu_facts}"
        );

        let divergences = compare(&gnu_facts, &facts);
        assert!(
            divergences.is_empty(),
            "TERM={term}: {} startup fact(s) differ from GNU:\n{}",
            divergences.len(),
            divergences.join("\n")
        );
        report.push(format!(
            "TERM={term}: -l ran, {} facts match GNU",
            facts.lines().count()
        ));
    }
    eprintln!("{}", report.join("\n"));
}

fn compare(gnu: &str, neomacs: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (left, right) in gnu.lines().zip(neomacs.lines()) {
        if left != right {
            out.push(format!("  GNU {left:?} vs Neomacs {right:?}"));
        }
    }
    if gnu.lines().count() != neomacs.lines().count() {
        out.push(format!(
            "  GNU recorded {} lines, neomacs {}",
            gnu.lines().count(),
            neomacs.lines().count()
        ));
    }
    out
}
