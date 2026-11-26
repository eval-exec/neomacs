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
        program: neomacs_tui_tests::oracle::gnu_oracle_program(),
        extra_args: vec![
            "-no-comp-spawn".to_string(),
            "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))".to_string(),
        ],
    }
}

/// Ensure `term` has a loadable terminfo description for both GNU and Neomacs.
///
/// macOS ships ncurses 5.7 terminfo which omits `alacritty` and `contour` (the
/// two `TERM` values in `xterm--auto-xt-mouse-allowed-types`). When the host
/// database lacks `term`, compile an isolated alias from `xterm-256color` via
/// `tic` so both editors exercise the real `TERM=alacritty` startup path.
///
/// Faithfulness: `infocmp -d xterm-256color alacritty` differs only on status
/// line (`hs`/`tsl`/`fsl`/`dsl`), `pairs` (65536 vs 32767), `blink`, `flash`,
/// `nel`, `kmous` (`\E[<` vs `\E[M`), margins (`mgc`/`smgl*`/`smgr*`), and
/// keypad aliases (`ka1`/`ka3`/`kb2`/`kbeg`/`kc1`/`kc3`). None of those are
/// read by `startup_terminal_initialization`: `term-file-aliases` (`faces.el`),
/// `xterm--auto-xt-mouse-allowed-types` (`xterm.el`), and `xt-mouse.el` key on
/// the `TERM` name and push hardcoded xterm mouse sequences, while the `init_tty`
/// capabilities (`cup`, `clear`, `sgr0`, `colors`, `setaf`/`setab`, `op`,
/// `smcup`/`rmcup`, `smkx`/`rmkx`, `cnorm`/`civis`) are identical.
fn ensure_terminfo_dir(term: &str, temp_root: &Path) -> Option<PathBuf> {
    let has_native = std::process::Command::new("infocmp")
        .arg(term)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if has_native {
        return None;
    }

    let base = std::process::Command::new("infocmp")
        .arg("xterm-256color")
        .output()
        .expect("run `infocmp xterm-256color` to synthesize missing terminfo entry");
    assert!(
        base.status.success(),
        "infocmp xterm-256color failed: {}",
        String::from_utf8_lossy(&base.stderr)
    );
    let desc = String::from_utf8(base.stdout).expect("xterm-256color terminfo UTF-8");
    let mut rewritten = String::with_capacity(desc.len() + term.len() + 64);
    let mut replaced_header = false;
    for line in desc.lines() {
        if !replaced_header && !line.is_empty() && !line.starts_with('#') && !line.starts_with('\t')
        {
            rewritten.push_str(&format!("{term}|{term} test alias of xterm-256color,\n"));
            replaced_header = true;
        } else {
            rewritten.push_str(line);
            rewritten.push('\n');
        }
    }
    assert!(
        replaced_header,
        "failed to locate terminfo header line in `infocmp xterm-256color`"
    );

    let terminfo_dir = temp_root.join("terminfo");
    std::fs::create_dir_all(&terminfo_dir).expect("create temp terminfo directory");
    let ti_path = temp_root.join(format!("{term}.ti"));
    std::fs::write(&ti_path, rewritten).expect("write synthesized terminfo source");
    let compiled = std::process::Command::new("tic")
        .arg("-o")
        .arg(&terminfo_dir)
        .arg(&ti_path)
        .output()
        .expect("run `tic` to compile synthesized terminfo entry");
    assert!(
        compiled.status.success(),
        "tic failed compiling synthesized {term} entry: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    Some(terminfo_dir)
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
    /// Which terminfo database satisfied `TERM` (`"native"` or synthesized).
    terminfo_source: &'static str,
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
    let terminfo_source = if let Some(terminfo_dir) = ensure_terminfo_dir(term, home.path()) {
        launch = launch.env("TERMINFO", terminfo_dir);
        "synthesized(xterm-256color)"
    } else {
        "native"
    };
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
        terminfo_source,
    }
}

fn read_facts(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().filter(|s| !s.is_empty())
}

#[test]
fn gnu_editor_uses_attested_oracle_program() {
    let editor = gnu();
    let expected = neomacs_tui_tests::oracle::gnu_oracle_program();
    assert_eq!(
        editor.program, expected,
        "startup_terminal_initialization must run the attested GNU oracle, not a raw PATH lookup"
    );
    assert!(
        editor.program.is_absolute(),
        "attested GNU oracle program must resolve to an absolute path, got {}",
        editor.program.display()
    );
    if let Some(forced) = std::env::var_os("NEOVM_FORCE_ORACLE_PATH") {
        let canonical_forced =
            std::fs::canonicalize(&forced).unwrap_or_else(|_| PathBuf::from(&forced));
        let canonical_actual =
            std::fs::canonicalize(&editor.program).unwrap_or_else(|_| editor.program.clone());
        assert_eq!(
            canonical_actual, canonical_forced,
            "GNU editor program must honor NEOVM_FORCE_ORACLE_PATH rather than substituting PATH emacs"
        );
    }
    assert!(
        !neomacs_tui_tests::oracle::gnu_oracle_stamp().is_empty(),
        "attested GNU oracle stamp must be non-empty"
    );
}

/// The gate: on every TERM here, `-l` must run, and it must record what GNU
/// records.
#[test]
fn startup_runs_dash_l_on_every_terminal_gnu_does() {
    let gnu_editor = gnu();
    let expected_oracle = neomacs_tui_tests::oracle::gnu_oracle_program();
    assert_eq!(
        gnu_editor.program, expected_oracle,
        "startup_runs_dash_l_on_every_terminal_gnu_does must run the attested GNU oracle"
    );
    let oracle_stamp = neomacs_tui_tests::oracle::gnu_oracle_stamp();
    let neomacs_editor = neomacs();
    let budget = Duration::from_secs(120);
    let mut report = vec![format!(
        "oracle: {} ({})",
        gnu_editor.program.display(),
        oracle_stamp
    )];

    for (term, auto_mouse) in TERMINALS {
        let gnu_run = run_startup(&gnu_editor, term, budget);
        let gnu_facts = gnu_run.facts.unwrap_or_else(|| {
            panic!(
                "GNU itself ({}) never ran -l on TERM={term} (terminfo={}); the harness is wrong, not neomacs",
                gnu_editor.program.display(),
                gnu_run.terminfo_source
            )
        });
        assert!(gnu_run.exited, "GNU did not exit on TERM={term}");

        let run = run_startup(&neomacs_editor, term, budget);
        let facts = run.facts.unwrap_or_else(|| {
            panic!(
                "neomacs swallowed -l on TERM={term} (terminfo={}, auto xterm-mouse: {auto_mouse}); \
                 startup aborted before command-line-1 processed the arguments. GNU recorded:\n{gnu_facts}",
                run.terminfo_source
            )
        });
        assert!(
            run.exited,
            "neomacs ran -l on TERM={term} but never exited; GNU recorded:\n{gnu_facts}"
        );

        let divergences = compare(&gnu_facts, &facts);
        assert!(
            divergences.is_empty(),
            "TERM={term} (terminfo={}): {} startup fact(s) differ from GNU:\n{}",
            run.terminfo_source,
            divergences.len(),
            divergences.join("\n")
        );
        report.push(format!(
            "TERM={term} [terminfo={}]: -l ran, {} facts match GNU",
            run.terminfo_source,
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
