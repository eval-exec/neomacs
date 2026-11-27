//! The cursor row across a C-n walk from line 98 to 100, GNU vs Neomacs.
//!
//! Written for #855, which it does not reproduce: no shape here draws the
//! cursor off its row. It stays as parity coverage of the cursor across its
//! line shapes and both backends.
//!
//! The reporter's third bundle narrowed #855 to point on the newline of a
//! short line, drawn in the row of the line below (`charpos=4686`, row
//! `chars=4688..4720`, a cursor-only redisplay). The `mid-*` cases put that
//! line in the middle of the window, so every step is a cursor-only redisplay:
//! an empty line, a one-character line, a lone trailing space, and a lone `)`
//! in `emacs-lisp-mode` with font-lock, plain and with Doom's line numbers,
//! hl-line, trailing whitespace and a taller mode line. None reproduces it
//! either, so what remains is in the reporter's configuration; their next
//! bundle, from the self-arming dump, decides it. The draw check now also
//! requires the cursor's position inside its row's char range, which the
//! reporter's line fails (`draw_defect_catches_a_cursor_resolved_into_another_row`).
//!
//! The report: C-n from line 98 to 99 draws the cursor at the start of line
//! 100, and a second C-n puts it on line 100 at the right column. The fixture
//! walks the same three steps in buffers whose line 99 is empty, short, exactly
//! the window width, one column narrower, and wrapped, with and without
//! line numbers, `hl-line-mode` and `visual-line-mode`. Two more put point on
//! a glyph twice and half the row's font height. The first C-n has to
//! scroll, because line 98 starts on the last body row.
//!
//! Two checks per step:
//! - Lisp-visible geometry (line, column, window start, and the posn and
//!   `window-cursor-info` rows) equals GNU 31.1's.
//! - On Neomacs, the drawn cursor (`neomacs::cursor_geometry` stage=draw)
//!   stays in layout's row at layout's rectangle. The renderer derives the
//!   drawn rectangle from the glyph in the cursor slot, so it can leave the
//!   row while every Lisp-visible value stays right, which is the symptom
//!   #855 reported. GNU draws the cursor from `w->phys_cursor` itself
//!   (xdisp.c `display_and_set_cursor`), so the two cannot differ there.
//!
//! GNU contract: `window-cursor-info` reads `w->phys_cursor` (window.c
//! `Fwindow_cursor_info`), set when redisplay draws the cursor in the row
//! that holds point (xdisp.c `set_cursor_from_row`, `display_and_set_cursor`).
//! `posn-at-point` (keyboard.c `Fposn_at_point`) locates point independently,
//! through `pos-visible-in-window-p`, so the two Ys check each other. Each
//! record is compared in rows of `default-line-height`, so the fonts of the
//! two editors' hosts do not decide the comparison.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// One settled observation, in the units both editors share.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WalkRecord {
    case: String,
    step: String,
    line: i64,
    column: i64,
    start_line: i64,
    /// `posn-at-point` Y in rows of `default-line-height`.
    posn_row: i64,
    /// `window-cursor-info` Y in rows of `default-line-height`.
    cursor_row: i64,
}

fn parse_records(output: &str) -> Vec<WalkRecord> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("CURSOR-ROW-WALK "))
        .map(|record| {
            let (head, info) = record
                .split_once(" [")
                .unwrap_or_else(|| panic!("no window-cursor-info vector in {record}"));
            let fields: Vec<&str> = head.split_whitespace().collect();
            let [case, step, line, column, start_line, line_height, posn_y] = fields[..] else {
                panic!("malformed record {record}");
            };
            let number = |text: &str| -> i64 {
                text.parse()
                    .unwrap_or_else(|_| panic!("{text:?} is not a number in {record}"))
            };
            let line_height = number(line_height);
            assert!(line_height > 0, "line height in {record}");
            let info: Vec<&str> = info.trim_end_matches(']').split_whitespace().collect();
            let cursor_y = number(info.get(2).unwrap_or_else(|| panic!("no Y in {record}")));
            WalkRecord {
                case: case.to_owned(),
                step: step.to_owned(),
                line: number(line),
                column: number(column),
                start_line: number(start_line),
                posn_row: number(posn_y).div_euclid(line_height),
                cursor_row: cursor_y.div_euclid(line_height),
            }
        })
        .collect()
}

fn observe(
    root: &Path,
    artifacts: &Path,
    backend: GuiBackend,
    binary: PathBuf,
    session_env: &[(String, String)],
) -> (Vec<WalkRecord>, String) {
    let fixture = root.join("crates/neomacs-gui-tests/fixtures/cursor-row-walk.el");
    let _ = fs::remove_dir_all(artifacts);
    fs::create_dir_all(artifacts).unwrap();
    let result_path = artifacts.join("probe-result.txt");
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        artifacts,
        GuiScenario::new("cursor-row-walk", &fixture),
    )
    .without_paint_proof()
    .with_program(binary)
    .with_args([
        "-Q".to_string(),
        "-l".to_string(),
        fixture.display().to_string(),
    ])
    .with_env(
        "NEOMACS_COMMIT_RESULT_FILE",
        result_path.to_string_lossy().into_owned(),
    )
    .with_env(
        "NEOMACS_LOG_FILE",
        artifacts.join("neomacs.log").to_string_lossy().into_owned(),
    )
    .with_env("RUST_LOG", "warn,neomacs::cursor_geometry=debug")
    .with_env("GSETTINGS_BACKEND", "memory");
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11");
    }
    for (key, value) in session_env {
        plan = plan.with_env(key, value);
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            // Explicit hang-avoidance cap only; the fixture exits once its
            // last case has been recorded.
            GuiRunOptions::with_timeout(Duration::from_secs(90)),
        )
        .unwrap();
    let stderr = fs::read_to_string(&result.artifacts.stderr).unwrap_or_default();
    let output = fs::read_to_string(&result_path).unwrap_or_default();
    let context = format!("{result:#?}\noutput:\n{output}\nstderr:\n{stderr}");
    assert!(!result.timed_out, "{context}");
    assert_eq!(result.exit_code, Some(0), "{context}");
    assert!(output.contains("COMMIT-EDITING-PASS"), "{context}");
    let records = parse_records(&output);
    assert_eq!(
        records.len(),
        CASES * 3,
        "{CASES} cases of three steps\n{context}"
    );
    (records, output)
}

/// The Neomacs draw stage of one step: where the renderer put the cursor.
#[derive(Debug)]
struct DrawRecord<'a> {
    case: &'a str,
    step: &'a str,
    line: &'a str,
}

/// `CASE STEP PHASE CURRENT WINDOW` per step phase, in order.
fn parse_current_buffers(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("CURSOR-ROW-WALK-CURRENT-BUFFER "))
        .collect()
}

fn parse_draws(output: &str) -> Vec<DrawRecord<'_>> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("CURSOR-ROW-WALK-DRAW "))
        .map(|record| {
            let mut parts = record.splitn(3, ' ');
            let case = parts.next().expect("case");
            let step = parts.next().expect("step");
            let line = parts
                .next()
                .unwrap_or_else(|| panic!("no draw line in {record}"));
            DrawRecord { case, step, line }
        })
        .collect()
}

/// The `y` of the `(x,y wxh)` rectangle after `key=(` in LINE.
fn rect_y(line: &str, key: &str) -> Option<f32> {
    let start = line.find(&format!(" {key}=("))? + key.len() + 3;
    let origin = line[start..].split(' ').next()?;
    origin.split_once(',')?.1.parse().ok()
}

/// The layout rectangle's `y=`, which follows `layout=(`.
fn layout_y(line: &str) -> Option<f32> {
    let layout = &line[line.find(" layout=(")?..];
    let start = layout.find(" y=")? + 3;
    layout[start..].split(' ').next()?.parse().ok()
}

/// Cases in the fixture's `neo-row-walk-cases`.
const CASES: usize = 25;

/// The cursor's buffer position and its row's `chars=START..END` range.
fn cursor_charpos_and_row_chars(line: &str) -> Option<(i64, i64, i64)> {
    let charpos = line
        .split(" charpos=")
        .nth(1)?
        .split(' ')
        .next()?
        .parse()
        .ok()?;
    let chars = line.split(" chars=").nth(1)?.split(' ').next()?;
    let (start, end) = chars.split_once("..")?;
    Some((charpos, start.parse().ok()?, end.parse().ok()?))
}

/// A drawn cursor must sit in layout's row, at layout's rectangle, and that
/// row must be the one holding the cursor's position. The last check is the
/// one the #855 reporter's log failed: `charpos=4686` drawn in a row with
/// `chars=4688..4720`, the line below point. A rectangle comparison alone
/// cannot see that.
fn draw_defect(record: &DrawRecord<'_>) -> Option<String> {
    let in_row = record.line.contains(" drawn_in_row=true ");
    let agrees = match (rect_y(record.line, "drawn"), layout_y(record.line)) {
        (Some(drawn), Some(layout)) => (drawn - layout).abs() < 1.0,
        _ => false,
    };
    let point_in_row = cursor_charpos_and_row_chars(record.line)
        .is_some_and(|(charpos, start, end)| start <= charpos && charpos <= end);
    (!(in_row && agrees && point_in_row))
        .then(|| format!("  {} {}: {}", record.case, record.step, record.line))
}

/// `window-cursor-info`'s Y for CASE and STEP, from the editor's own record.
fn cursor_info_record<'a>(output: &'a str, case: &str, step: &str) -> Option<(f32, &'a str)> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("CURSOR-ROW-WALK "))
        .find_map(|record| {
            let at = record.find(" [")?;
            let fields: Vec<&str> = record[..at].split_whitespace().collect();
            let vector = record[at + 1..].trim_end();
            let y = vector.split_whitespace().nth(2)?.parse().ok()?;
            (fields.first() == Some(&case) && fields.get(1) == Some(&step)).then_some((y, vector))
        })
}

/// The `(x,y wxh)` text after `key=` in LINE, for failure messages.
fn rect_text<'a>(line: &'a str, key: &str) -> &'a str {
    line.split(&format!(" {key}="))
        .nth(1)
        .map_or("missing", |rest| {
            rest.find(')').map_or(rest, |end| &rest[..=end])
        })
}

/// In the same frame, the drawn cursor must be the physical cursor Lisp
/// reports: drawn y minus the text area's top equals `window-cursor-info`'s
/// Y. Together with the per-step GNU comparison of the `posn-at-point` and
/// `window-cursor-info` rows, this ties the drawn cursor to the row
/// `posn-at-point` gives point. #855 was confirmed by posn and the drawn
/// cursor disagreeing by a row while the drawn and layout rectangles agreed.
/// (`posn-at-point`'s Y is measured below a header line and
/// `window-cursor-info`'s from the window's text-area top, on GNU as well, so
/// the two are compared through GNU rather than to each other directly.)
fn cursor_info_defect(record: &DrawRecord<'_>, neo_output: &str) -> Option<String> {
    let info = cursor_info_record(neo_output, record.case, record.step);
    // Both rectangles go into every failure: the drawn cursor with the text
    // area it is measured in, and the `window-cursor-info' vector Lisp saw.
    let rects = || {
        format!(
            "drawn={} text_area={} window-cursor-info={}",
            rect_text(record.line, "drawn"),
            rect_text(record.line, "text_area"),
            info.map_or("missing", |(_, vector)| vector)
        )
    };
    // Fail closed: a step whose numbers cannot be read is a defect, never a
    // silent pass.
    let (Some(drawn), Some(text_top), Some((info_y, _))) = (
        rect_y(record.line, "drawn"),
        rect_y(record.line, "text_area"),
        info,
    ) else {
        return Some(format!(
            "  {} {}: unreadable step: {}\n    {}",
            record.case,
            record.step,
            rects(),
            record.line
        ));
    };
    ((drawn - text_top - info_y).abs() >= 1.0).then(|| {
        format!(
            "  {} {}: drawn y - text top = {} but window-cursor-info Y = {info_y}: {}\n    {}",
            record.case,
            record.step,
            drawn - text_top,
            rects(),
            record.line
        )
    })
}

fn walk_matches_gnu(neo_backend: GuiBackend) {
    let root = neomacs_infra::workspace_root();
    let artifacts = root
        .join("target/neomacs-gui-tests")
        .join("cursor-row-walk")
        .join(neo_backend.slug());
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "set NEOVM_FORCE_ORACLE_PATH to the pinned GUI oracle"
    );
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));

    let gnu_session = DisplayHarness::for_backend(GuiBackend::LinuxX11)
        .start_session(&artifacts.join("gnu-session"))
        .unwrap();
    let (gnu, gnu_output) = observe(
        &root,
        &artifacts.join("gnu"),
        GuiBackend::LinuxX11,
        oracle.executable_path(),
        &gnu_session.env(),
    );
    drop(gnu_session);
    let neo_session = DisplayHarness::for_backend(neo_backend)
        .start_session(&artifacts.join("neo-session"))
        .unwrap();
    let (actual, neo_output) = observe(
        &root,
        &artifacts.join("neo"),
        neo_backend,
        neo,
        &neo_session.env(),
    );

    // Timer-chain parity: each phase starts in a fresh timer callback, so
    // GNU's timer-event-handler has restored the buffer that was current
    // before the chain (*scratch*), while the window shows the case's
    // buffer. Neomacs must do exactly the same; a "fix" that made the
    // callbacks start in the window's buffer would diverge from GNU.
    let gnu_buffers = parse_current_buffers(&gnu_output);
    let neo_buffers = parse_current_buffers(&neo_output);
    assert_eq!(
        gnu_buffers.len(),
        CASES * 3 * 2,
        "two phases per step on GNU"
    );
    assert!(
        gnu_buffers
            .iter()
            .all(|line| line.contains(" *scratch* row-walk-")),
        "GNU 31.1 starts each chained timer in *scratch*: {gnu_buffers:#?}"
    );
    assert_eq!(
        neo_buffers, gnu_buffers,
        "chained-timer current-buffer differs from GNU 31.1"
    );

    let draws = parse_draws(&neo_output);
    assert_eq!(draws.len(), gnu.len(), "one Neomacs draw line per step");
    let misdrawn: Vec<String> = draws
        .iter()
        .filter_map(|draw| draw_defect(draw).or_else(|| cursor_info_defect(draw, &neo_output)))
        .collect();
    let diverging: Vec<String> = gnu
        .iter()
        .zip(&actual)
        .filter(|(gnu, neo)| gnu != neo)
        .map(|(gnu, neo)| format!("  gnu {gnu:?}\n  neo {neo:?}"))
        .collect();
    assert!(
        diverging.is_empty() && misdrawn.is_empty(),
        "{} of {} observations differ from GNU 31.1, and {} drawn cursors leave \
         layout's row, point's row or the physical cursor Lisp reports (Neomacs log: {}):\n{}\n\
         misdrawn:\n{}",
        diverging.len(),
        gnu.len(),
        misdrawn.len(),
        artifacts.join("neo/neomacs.log").display(),
        diverging.join("\n"),
        misdrawn.join("\n")
    );
}

#[test]
fn cursor_row_walk_matches_gnu_on_x11() {
    walk_matches_gnu(GuiBackend::LinuxX11);
}

#[test]
fn cursor_row_walk_matches_gnu_on_wayland() {
    walk_matches_gnu(GuiBackend::LinuxWayland);
}

/// The #855 reporter's draw line (bundle on e505d3d845) must count as a
/// defect: its rectangles agree, but `charpos=4686` lies before its row's
/// `chars=4688..4720`. A well-formed line from this walk must not.
#[test]
fn draw_defect_catches_a_cursor_resolved_into_another_row() {
    let reporter = "frame=4294967297 stage=draw presentation=295 window=3 charpos=4686 \
        style=FilledBox layout=(row=18 col=5 slot=18,5 x=63.0 y=475.0 w=11.0 h=25.0 \
        ascent=19.0) drawn=(63.0,475.0 11.0x25.0) drawn_in_row=true \
        text_area=(8.0,25.0 1859.0x825.0) row=(role=Text top=475.0 h=25.0 pixel_y=450.0 \
        ascent=19.0 spacing=0.0 chars=4688..4720 cursor_col=5 damage=New) glyph=char(' ' \
        x=63.0 y=475.0 baseline=494.0 w=11.0 h=25.0 ascent=19.0 face=0)";
    let ours = "frame=4294967297 stage=draw presentation=488 window=3 charpos=1943 \
        style=FilledBox layout=(row=18 col=6 slot=18,6 x=56.0 y=357.0 w=8.0 h=17.0 \
        ascent=13.0) drawn=(56.0,357.0 8.0x17.0) drawn_in_row=true \
        text_area=(8.0,51.0 640.0x578.0) row=(role=Text top=357.0 h=17.0 pixel_y=306.0 \
        ascent=13.0 spacing=0.0 chars=1942..1943 cursor_col=6 damage=Reused) glyph=missing";
    let record = |line| DrawRecord {
        case: "case",
        step: "step",
        line,
    };
    assert!(draw_defect(&record(reporter)).is_some());
    assert!(draw_defect(&record(ours)).is_none());
}

/// A drawn cursor one row away from the physical cursor Lisp reports is a
/// defect even when its rectangles agree with layout and its row holds point;
/// the same draw line with `window-cursor-info` at drawn y - text top is not.
#[test]
fn cursor_info_defect_catches_a_drawn_cursor_away_from_window_cursor_info() {
    let line = "frame=4294967297 stage=draw presentation=488 window=3 charpos=1943 \
        style=FilledBox layout=(row=18 col=6 slot=18,6 x=56.0 y=357.0 w=8.0 h=17.0 \
        ascent=13.0) drawn=(56.0,357.0 8.0x17.0) drawn_in_row=true \
        text_area=(8.0,51.0 640.0x578.0) row=(role=Text top=357.0 h=17.0 pixel_y=306.0 \
        ascent=13.0 spacing=0.0 chars=1942..1943 cursor_col=6 damage=Reused) glyph=missing";
    let record = DrawRecord {
        case: "mid-one-char-doom",
        step: "1",
        line,
    };
    // drawn y 357 - text-area top 51 = 306.
    let agrees = "CURSOR-ROW-WALK mid-one-char-doom 1 99 1 80 17 306 [t 48 306 8 17 13]\n";
    let a_row_above = "CURSOR-ROW-WALK mid-one-char-doom 1 99 1 80 17 289 [t 48 289 8 17 13]\n";
    assert_eq!(cursor_info_defect(&record, agrees), None);
    let defect = cursor_info_defect(&record, a_row_above).expect("a row above is a defect");
    assert!(
        defect.contains("drawn=(56.0,357.0 8.0x17.0)")
            && defect.contains("text_area=(8.0,51.0 640.0x578.0)")
            && defect.contains("window-cursor-info=[t 48 289 8 17 13]"),
        "the failure names both rectangles: {defect}"
    );
    assert!(
        cursor_info_defect(&record, "").is_some(),
        "a step without its window-cursor-info record fails closed"
    );
}
