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

/// Upstream ncurses `alacritty` terminfo source (generated via `infocmp -1 alacritty`
/// from ncurses 6.x). Used verbatim when the host terminfo database omits `alacritty`
/// (such as macOS's ncurses 5.7 `/usr/share/terminfo`) so both GNU Emacs and Neomacs
/// run against the real `alacritty` capability set rather than an `xterm-256color` alias.
const UPSTREAM_ALACRITTY_TERMINFO: &str = r##"alacritty|alacritty terminal emulator,
	am,
	bce,
	ccc,
	hs,
	km,
	mc5i,
	mir,
	msgr,
	npc,
	xenl,
	colors#0x100,
	cols#80,
	it#8,
	lines#24,
	pairs#0x7fff,
	acsc=``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~,
	bel=^G,
	bold=\E[1m,
	cbt=\E[Z,
	civis=\E[?25l,
	clear=\E[H\E[2J,
	cnorm=\E[?12l\E[?25h,
	cr=\r,
	csr=\E[%i%p1%d;%p2%dr,
	cub=\E[%p1%dD,
	cub1=^H,
	cud=\E[%p1%dB,
	cud1=\n,
	cuf=\E[%p1%dC,
	cuf1=\E[C,
	cup=\E[%i%p1%d;%p2%dH,
	cuu=\E[%p1%dA,
	cuu1=\E[A,
	cvvis=\E[?12;25h,
	dch=\E[%p1%dP,
	dch1=\E[P,
	dim=\E[2m,
	dl=\E[%p1%dM,
	dl1=\E[M,
	dsl=\E]2;\007,
	ech=\E[%p1%dX,
	ed=\E[J,
	el=\E[K,
	el1=\E[1K,
	fsl=^G,
	home=\E[H,
	hpa=\E[%i%p1%dG,
	ht=^I,
	hts=\EH,
	ich=\E[%p1%d@,
	il=\E[%p1%dL,
	il1=\E[L,
	ind=\n,
	indn=\E[%p1%dS,
	initc=\E]4;%p1%d;rgb:%p2%{255}%*%{1000}%/%2.2X/%p3%{255}%*%{1000}%/%2.2X/%p4%{255}%*%{1000}%/%2.2X\E\\,
	invis=\E[8m,
	is2=\E[!p\E[?3;4l\E[4l\E>,
	kDC=\E[3;2~,
	kEND=\E[1;2F,
	kHOM=\E[1;2H,
	kIC=\E[2;2~,
	kLFT=\E[1;2D,
	kNXT=\E[6;2~,
	kPRV=\E[5;2~,
	kRIT=\E[1;2C,
	kb2=\EOE,
	kbs=^?,
	kcbt=\E[Z,
	kcub1=\EOD,
	kcud1=\EOB,
	kcuf1=\EOC,
	kcuu1=\EOA,
	kdch1=\E[3~,
	kend=\EOF,
	kent=\EOM,
	kf1=\EOP,
	kf10=\E[21~,
	kf11=\E[23~,
	kf12=\E[24~,
	kf13=\E[1;2P,
	kf14=\E[1;2Q,
	kf15=\E[1;2R,
	kf16=\E[1;2S,
	kf17=\E[15;2~,
	kf18=\E[17;2~,
	kf19=\E[18;2~,
	kf2=\EOQ,
	kf20=\E[19;2~,
	kf21=\E[20;2~,
	kf22=\E[21;2~,
	kf23=\E[23;2~,
	kf24=\E[24;2~,
	kf25=\E[1;5P,
	kf26=\E[1;5Q,
	kf27=\E[1;5R,
	kf28=\E[1;5S,
	kf29=\E[15;5~,
	kf3=\EOR,
	kf30=\E[17;5~,
	kf31=\E[18;5~,
	kf32=\E[19;5~,
	kf33=\E[20;5~,
	kf34=\E[21;5~,
	kf35=\E[23;5~,
	kf36=\E[24;5~,
	kf37=\E[1;6P,
	kf38=\E[1;6Q,
	kf39=\E[1;6R,
	kf4=\EOS,
	kf40=\E[1;6S,
	kf41=\E[15;6~,
	kf42=\E[17;6~,
	kf43=\E[18;6~,
	kf44=\E[19;6~,
	kf45=\E[20;6~,
	kf46=\E[21;6~,
	kf47=\E[23;6~,
	kf48=\E[24;6~,
	kf49=\E[1;3P,
	kf5=\E[15~,
	kf50=\E[1;3Q,
	kf51=\E[1;3R,
	kf52=\E[1;3S,
	kf53=\E[15;3~,
	kf54=\E[17;3~,
	kf55=\E[18;3~,
	kf56=\E[19;3~,
	kf57=\E[20;3~,
	kf58=\E[21;3~,
	kf59=\E[23;3~,
	kf6=\E[17~,
	kf60=\E[24;3~,
	kf61=\E[1;4P,
	kf62=\E[1;4Q,
	kf63=\E[1;4R,
	kf7=\E[18~,
	kf8=\E[19~,
	kf9=\E[20~,
	khome=\EOH,
	kich1=\E[2~,
	kind=\E[1;2B,
	kmous=\E[M,
	knp=\E[6~,
	kpp=\E[5~,
	kri=\E[1;2A,
	mc0=\E[i,
	mc4=\E[4i,
	mc5=\E[5i,
	meml=\El,
	memu=\Em,
	oc=\E]104\007,
	op=\E[39;49m,
	rc=\E8,
	rep=%p1%c\E[%p2%{1}%-%db,
	rev=\E[7m,
	ri=\EM,
	rin=\E[%p1%dT,
	ritm=\E[23m,
	rmacs=\E(B,
	rmam=\E[?7l,
	rmcup=\E[?1049l\E[23;0;0t,
	rmir=\E[4l,
	rmkx=\E[?1l\E>,
	rmm=\E[?1034l,
	rmso=\E[27m,
	rmul=\E[24m,
	rs1=\Ec\E]104\007,
	rs2=\E[!p\E[?3;4l\E[4l\E>,
	sc=\E7,
	setab=\E[%?%p1%{8}%<%t4%p1%d%e%p1%{16}%<%t10%p1%{8}%-%d%e48;5;%p1%d%;m,
	setaf=\E[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m,
	setb@,
	setf@,
	sgr=%?%p9%t\E(0%e\E(B%;\E[0%?%p6%t;1%;%?%p5%t;2%;%?%p2%t;4%;%?%p1%p3%|%t;7%;%?%p4%t;5%;%?%p7%t;8%;m,
	sgr0=\E(B\E[m,
	sitm=\E[3m,
	smacs=\E(0,
	smam=\E[?7h,
	smcup=\E[?1049h\E[22;0;0t,
	smir=\E[4h,
	smkx=\E[?1h\E=,
	smm=\E[?1034h,
	smso=\E[7m,
	smul=\E[4m,
	tbc=\E[3g,
	tsl=\E]2;,
	u6=\E[%i%d;%dR,
	u7=\E[6n,
	u8=\E[?%[;0123456789]c,
	u9=\E[c,
	vpa=\E[%i%p1%dd,
"##;

/// Ensure `term` has a loadable terminfo description for both GNU and Neomacs.
///
/// macOS ships ncurses 5.7 terminfo which omits `alacritty` and `contour` (the
/// two `TERM` values in `xterm--auto-xt-mouse-allowed-types`). When the host
/// database lacks `alacritty`, compile the verbatim upstream `alacritty`
/// terminfo source via `tic` into a test-scoped directory so both editors test
/// the real `alacritty` capability set.
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

    let source = match term {
        "alacritty" => UPSTREAM_ALACRITTY_TERMINFO,
        other => panic!(
            "host terminfo lacks {other:?} and no upstream terminfo source is embedded for it"
        ),
    };
    let terminfo_dir = temp_root.join("terminfo");
    std::fs::create_dir_all(&terminfo_dir).expect("create temp terminfo directory");
    let ti_path = temp_root.join(format!("{term}.ti"));
    std::fs::write(&ti_path, source).expect("write upstream terminfo source");
    let compiled = std::process::Command::new("tic")
        .arg("-o")
        .arg(&terminfo_dir)
        .arg(&ti_path)
        .output()
        .expect("run `tic` to compile upstream terminfo entry");
    assert!(
        compiled.status.success(),
        "tic failed compiling upstream {term} entry: {}",
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
        "embedded-upstream-alacritty"
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
