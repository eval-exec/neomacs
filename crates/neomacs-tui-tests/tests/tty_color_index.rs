//! The colour a face reaches the terminal as is the one Lisp computed.
//!
//! GNU never quantizes in the writer.  `map_tty_color` (src/xfaces.c:6620-6694)
//! takes the INDEX part of `tty-color-desc`'s `(NAME INDEX R G B)` into the
//! realized face, and `turn_on_face` (src/term.c:2093-2117) hands exactly that
//! number to terminfo `setaf`/`setab`.  The palette that number came from is
//! `tty-color-alist`: Lisp data, registered per terminal by `lisp/term/<TERM>.el`
//! and modifiable at any time by `tty-color-define`
//! (lisp/term/tty-colors.el:840-861).
//!
//! These two suites gate the two halves of that.  The first drives the SEARCH,
//! over the whole RGB cube on four terminals whose palettes genuinely differ;
//! the second drives the PLUMBING, comparing the bytes each editor actually
//! writes for a face -- including after `tty-color-define` has moved a colour to
//! an index no RGB search could reach.
//!
//! Both spawn GNU as the oracle rather than pinning a fixture, so they cover any
//! terminal the machine has an entry for and cannot go stale.  The RECORDED GNU
//! answers -- ledger 153's 5,832-sample sweep, now on four terminals -- live
//! next to the search they gate, in
//! `crates/neomacs-display-protocol/src/tty_palette_data/`.

use std::ffi::OsStr;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Every terminal here reports a DIFFERENT palette, which is the point:
///
/// ```text
/// TERM=xterm            display-color-cells   8   tty-color-alist   8 entries
/// TERM=rxvt-16color     display-color-cells  16   tty-color-alist  16 entries
/// TERM=linux-16color    display-color-cells  16   tty-color-alist   8 entries
/// TERM=xterm-256color   display-color-cells 256   tty-color-alist 256 entries
/// ```
///
/// `rxvt-16color` is the row no fixed table can serve: its `blue` is (0,0,205)
/// where xterm's is (0,0,238), its `brightblack` (77,77,77) against
/// (127,127,127), its `brightblue` (0,0,255) against (92,92,255).
/// `linux-16color` is the row that shows the cell count and the palette are two
/// different facts.
const SWEEP_TERMINALS: [&str; 4] = ["xterm", "rxvt-16color", "linux-16color", "xterm-256color"];

/// 18 values per channel over 0..255, 18^3 = 5,832 samples.
const SWEEP_EL: &str = r##"
(with-temp-file (getenv "NEOMACS_TTY_COLOR_OUT")
  (insert (format "# CELLS %s ENTRIES %s\n"
                  (display-color-cells) (length (tty-color-alist))))
  (dotimes (ri 18)
    (dotimes (gi 18)
      (dotimes (bi 18)
        (let ((r (* ri 15)) (g (* gi 15)) (b (* bi 15)))
          (insert (format "%02x%02x%02x %s\n" r g b
                          (nth 1 (tty-color-approximate
                                  (list (* r 257) (* g 257) (* b 257)))))))))))
(kill-emacs)
"##;

/// Six faces, then the SGR each one made it onto the wire as.
///
/// `pw-name` under `NEOMACS_TTY_COLOR_DEFINE` is the case only a carried index
/// can serve: `tty-color-define` moves the NAME "red" to palette slot 5, which
/// `map_tty_color` finds by `assoc` (src/xfaces.c:6645-6647) without
/// approximating anything.  A writer that re-derives the index from (255,0,0)
/// answers whatever that approximates to -- 1 on an 8-colour terminal, 9
/// elsewhere -- and no palette data fixes that, because the answer was never a
/// function of the RGB.  Slot 5 is inside every palette here, so the comparison
/// stays about the INDEX and not about how a terminal spells one it cannot hold.
///
/// `pw-hex` is the 16-colour palette case: `#0000ff` is an EXACT `rxvt-16color`
/// `brightblue`, and nothing but that terminal's own alist knows it.
///
/// The three `plist` rows are anonymous attribute plists, which GNU folds into
/// the same lface vector and realizes through the same `map_tty_color`, and
/// which neomacs realizes in a different crate entirely.
const PROBE_EL: &str = r##"
(setq inhibit-startup-screen t inhibit-message t)
(when (getenv "NEOMACS_TTY_COLOR_DEFINE")
  (tty-color-define "red" 5 '(65535 0 0))
  (clear-face-cache))
(defface pw-name '((t :foreground "red")) "probe")
(defface pw-hex '((t :foreground "#0000ff")) "probe")
(defface pw-gray '((t :foreground "#4d4d4d")) "probe")
(let ((b (get-buffer-create "*pw*")))
  (with-current-buffer b
    (erase-buffer)
    (insert "AAA" (propertize "ZZZZZZ" 'face 'pw-name) "AAA\n")
    (insert "BBB" (propertize "YYYYYY" 'face 'pw-hex) "BBB\n")
    (insert "CCC" (propertize "XXXXXX" 'face 'pw-gray) "CCC\n")
    (insert "DDD" (propertize "WWWWWW" 'face '(:foreground "#5f8787")) "DDD\n")
    (insert "EEE" (propertize "VVVVVV" 'face '(:foreground "red")) "EEE\n")
    (insert "FFF" (propertize "UUUUUU" 'face '(:background "#3a3a3a")) "FFF\n"))
  (switch-to-buffer b))
(run-with-timer 2 nil #'kill-emacs)
"##;

/// Marker text, and what face put it there.
const PROBE_MARKERS: [(&str, &str); 6] = [
    ("ZZZZZZ", "face  :foreground \"red\""),
    ("YYYYYY", "face  :foreground \"#0000ff\""),
    ("XXXXXX", "face  :foreground \"#4d4d4d\""),
    ("WWWWWW", "plist (:foreground \"#5f8787\")"),
    ("VVVVVV", "plist (:foreground \"red\")"),
    ("UUUUUU", "plist (:background \"#3a3a3a\")"),
];

struct Editor {
    name: &'static str,
    program: PathBuf,
    /// GNU's async native compiler can pop *Warnings* mid-run; keep it quiet so
    /// the captured bytes are the probe's and nothing else's.
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

/// Run one editor to completion on a real pty of `term`, and return every byte
/// it wrote to that pty.
///
/// The pty is sized explicitly: a terminal reporting 0x0 never lays anything
/// out, and the probe suite reads the bytes a real redisplay produced.
/// `COLORTERM` is removed, because it alone can make `display-color-cells`
/// 16777216 (GNU `init_tty`, src/term.c:4655-4665) and turn every indexed answer
/// into a packed pixel.
fn run_on_pty(
    editor: &Editor,
    term: &str,
    elisp: &str,
    environment: &[(&str, &OsStr)],
    budget: Duration,
) -> Vec<u8> {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-tty-color-home-");
    let script = home.path().join("probe.el");
    std::fs::write(&script, elisp).expect("write probe elisp");

    let (pty, pts) = pty_process::blocking::open().expect("open pty");
    pty.resize(pty_process::Size::new(24, 80)).expect("resize");

    let mut command = pty_process::blocking::Command::new(&editor.program);
    command = command.arg("-nw").arg("-Q");
    for arg in &editor.extra_args {
        command = command.arg(arg);
    }
    command = command
        .arg("-l")
        .arg(&script)
        .env("TERM", term)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("LINES", "24")
        .env("COLUMNS", "80")
        .env_remove("COLORTERM");
    for (name, value) in environment {
        command = command.env(name, value);
    }
    let mut child = command.spawn(pts).expect("spawn editor on pty");

    // Drain the pty on another thread: a child that fills the pty buffer while
    // the parent waits for it to exit deadlocks with itself.
    let reader = std::thread::spawn(move || {
        let mut pty = pty;
        let mut buffer = Vec::new();
        let _ = pty.read_to_end(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + budget;
    loop {
        match child.try_wait().expect("wait on editor") {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{} did not finish on TERM={term} within {budget:?}",
                    editor.name
                );
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    reader.join().expect("pty reader thread")
}

fn sweep(editor: &Editor, term: &str) -> String {
    let out = neomacs_tui_tests::TuiTempDirectory::new("neomacs-tty-color-out-");
    let path = out.path().join("sweep.txt");
    run_on_pty(
        editor,
        term,
        SWEEP_EL,
        &[("NEOMACS_TTY_COLOR_OUT", path.as_os_str())],
        Duration::from_secs(300),
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} wrote no sweep on TERM={term}: {error}", editor.name));
    let samples = text.lines().filter(|line| !line.starts_with('#')).count();
    assert_eq!(
        samples, 5832,
        "{} lost sweep samples on TERM={term}",
        editor.name
    );
    text
}

fn differences(gnu: &str, neomacs: &str, limit: usize) -> (usize, String) {
    let mut count = 0;
    let mut shown = Vec::new();
    for (left, right) in gnu.lines().zip(neomacs.lines()) {
        if left != right {
            count += 1;
            if shown.len() < limit {
                shown.push(format!("GNU {left:?} vs Neomacs {right:?}"));
            }
        }
    }
    (count, shown.join("\n"))
}

/// `tty-color-approximate` over the whole RGB cube, on four terminals whose
/// palettes differ, must answer exactly what GNU answers.
///
/// This is ledger 153's 5,832-colour gate moved onto the production path.  It
/// used to compare a Rust reimplementation of the search against a recorded
/// fixture; the search that decides a face's colour is now
/// `lisp/term/tty-colors.el:875-915` itself, byte-identical to GNU's, and the
/// only way it can diverge is if the PALETTE differs -- which is precisely what
/// ledger 153 measured going wrong (18.2% of these very samples on
/// `rxvt-16color`, 40.6% on `linux-16color`) while the search was exact.
#[test]
fn tty_color_approximate_matches_gnu_over_the_whole_rgb_cube() {
    let gnu_editor = gnu();
    let neomacs_editor = neomacs();
    let mut report = Vec::new();
    for term in SWEEP_TERMINALS {
        let gnu_answers = sweep(&gnu_editor, term);
        let neomacs_answers = sweep(&neomacs_editor, term);
        let (mismatches, sample) = differences(&gnu_answers, &neomacs_answers, 8);
        assert_eq!(
            mismatches, 0,
            "TERM={term}: {mismatches} of 5832 colours differ from GNU\n{sample}"
        );
        report.push(format!("TERM={term}: 0 of 5832 differ"));
    }
    eprintln!("{}", report.join("\n"));
}

/// The SGR each editor writes for the same six faces, on three terminals, with
/// and without a `tty-color-define` that moves a name to another slot.
///
/// The `define` half is the one no writer-side search can pass, and the three
/// `plist` rows are the ones no named-face path reaches.
#[test]
fn face_colours_reach_the_wire_as_the_index_lisp_computed() {
    let gnu_editor = gnu();
    let neomacs_editor = neomacs();
    let mut divergences = Vec::new();
    let mut compared = 0_usize;
    for term in ["xterm", "rxvt-16color", "xterm-256color"] {
        for define in [false, true] {
            let environment: Vec<(&str, &OsStr)> = if define {
                vec![("NEOMACS_TTY_COLOR_DEFINE", OsStr::new("1"))]
            } else {
                Vec::new()
            };
            let budget = Duration::from_secs(120);
            let gnu_bytes = run_on_pty(&gnu_editor, term, PROBE_EL, &environment, budget);
            let neomacs_bytes = run_on_pty(&neomacs_editor, term, PROBE_EL, &environment, budget);
            for (marker, face) in PROBE_MARKERS {
                let gnu_sgr = color_sgr_before(&gnu_bytes, marker).unwrap_or_else(|| {
                    panic!("GNU never coloured {marker} on TERM={term} (define={define})")
                });
                let neomacs_sgr = color_sgr_before(&neomacs_bytes, marker).unwrap_or_else(|| {
                    panic!("Neomacs never coloured {marker} on TERM={term} (define={define})")
                });
                compared += 1;
                if gnu_sgr != neomacs_sgr {
                    divergences.push(format!(
                        "TERM={term} define={define} {face}: GNU {gnu_sgr:?}, Neomacs {neomacs_sgr:?}"
                    ));
                }
            }
        }
    }
    assert_eq!(
        compared, 36,
        "every face must be measured on every terminal"
    );
    assert!(
        divergences.is_empty(),
        "the wire disagrees with GNU:\n{}",
        divergences.join("\n")
    );
}

/// The last SGR that SELECTS a colour before `marker` is drawn.
///
/// The two editors reach the same cell by different cursor paths and reset
/// differently, so the comparison is of the colour each one selected, not of the
/// whole byte stream.  `39`/`49` are the default-colour resets, which both
/// editors emit constantly and neither of which selects anything.
fn color_sgr_before(stream: &[u8], marker: &str) -> Option<String> {
    let at = find_subslice(stream, marker.as_bytes())?;
    let window = &stream[at.saturating_sub(400)..at];
    let mut last = None;
    let mut index = 0;
    while index + 1 < window.len() {
        if window[index] == 0x1b && window[index + 1] == b'[' {
            let mut end = index + 2;
            while end < window.len() && window[end] != b'm' && window[end].is_ascii_graphic() {
                end += 1;
            }
            if end < window.len() && window[end] == b'm' {
                let parameters = String::from_utf8_lossy(&window[index + 2..end]).into_owned();
                if selects_a_colour(&parameters) {
                    last = Some(parameters);
                }
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }
    last
}

/// Whether an SGR parameter list selects a foreground or background colour, as
/// opposed to resetting one or setting some other attribute.
fn selects_a_colour(parameters: &str) -> bool {
    if parameters.starts_with("38;") || parameters.starts_with("48;") {
        return true;
    }
    parameters
        .split(';')
        .filter_map(|part| part.parse::<u32>().ok())
        .any(|code| {
            (30..=37).contains(&code)
                || (90..=97).contains(&code)
                || (40..=47).contains(&code)
                || (100..=107).contains(&code)
        })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
