#![cfg(unix)]
//! Regression coverage for issue #441: `global-display-line-numbers-mode`
//! with `display-line-numbers-type` set to `absolute` must light the gutter
//! in buffers created AFTER the global mode enabled — the file buffer the
//! issue visits — not only in the buffers that existed at init time (the
//! *Messages* the reporter saw).
//!
//! The fixture is the issue's exact init: enable the global mode, then visit
//! a file. GNU renders `N  text` gutters in the visited buffer; the Neomacs
//! divergence is either the globalized-minor-mode's new-buffer enabler never
//! firing (the local mode stays off) or the display engine ignoring the
//! mode's `display-line-numbers` variable — a session-side probe tells those
//! apart during the fix, and the exact-pair match pins the rendered geometry.

use crate::support;

use neomacs_tui_tests::TuiTempFile;
use std::time::Duration;

use support::{boot_pair, dump_pair_grids, read_both};

/// The issue's init, verbatim, plus a visit of a multi-line file so the
/// first frame after startup displays a buffer created after the global
/// mode turned on.
fn init_fixture(visited: &std::path::Path) -> TuiTempFile {
    TuiTempFile::new(
        "neomacs-issue-441",
        "init.el",
        format!(
            ";;; issue-441 init -*- lexical-binding: t; -*-\n\
             (global-display-line-numbers-mode 1)\n\
             (setq display-line-numbers-type 'absolute)\n\
             (find-file {visited:?})\n",
            visited = visited.display().to_string(),
        ),
    )
}

const BODY: &str = "alpha line";
const TAIL: &str = "last line";

#[test]
fn display_string_newlines_do_not_number_synthetic_rows_or_shift_buffer_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    support::eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
          (erase-buffer)
          (dotimes (i 5) (insert (format "line %d\n" (1+ i))))
          (display-line-numbers-mode 1)
          (goto-char (point-min))
          (let ((eol (line-end-position)))
            (overlay-put (make-overlay eol (1+ eol)) 'display "  [A]\n  [B]\n"))
          (forward-line 2)
          nil)"##,
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let rows = |grid: Vec<String>| {
        grid.into_iter()
            .filter(|row| row.contains("line ") || row.contains("[B]"))
            .map(|row| row.trim().to_owned())
            .collect::<Vec<_>>()
    };
    let expected = [
        "1 line 1  [A]",
        "[B]",
        "2 line 2",
        "3 line 3",
        "4 line 4",
        "5 line 5",
    ];
    assert_eq!(rows(gnu.text_grid()), expected, "GNU 31.1 oracle");
    assert_eq!(rows(neo.text_grid()), expected, "issue #574");

    support::eval_expression(&mut gnu, &mut neo, "(setq display-line-numbers 'relative)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let relative = [
        "2 line 1  [A]",
        "[B]",
        "1 line 2",
        "3 line 3",
        "1 line 4",
        "2 line 5",
    ];
    assert_eq!(rows(gnu.text_grid()), relative, "GNU relative-mode oracle");
    assert_eq!(
        rows(neo.text_grid()),
        relative,
        "relative display-string rows"
    );
}

#[test]
fn display_replacement_counts_the_buffer_newlines_it_hides() {
    let (mut gnu, mut neo) = boot_pair("");
    support::eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
          (erase-buffer)
          (dotimes (i 5) (insert (format "line %d\n" (1+ i))))
          (display-line-numbers-mode 1)
          (goto-char (point-min))
          (forward-line 1)
          (put-text-property (point) (progn (forward-line 2) (point)) 'display "[hidden] ")
          (goto-char (point-max))
          (forward-line -1)
          nil)"##,
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let rows = |grid: Vec<String>| {
        grid.into_iter()
            .filter(|row| row.contains("line "))
            .map(|row| row.trim().to_owned())
            .collect::<Vec<_>>()
    };
    let expected = ["1 line 1", "2 [hidden] line 4", "5 line 5"];
    assert_eq!(rows(gnu.text_grid()), expected, "GNU 31.1 oracle");
    assert_eq!(
        rows(neo.text_grid()),
        expected,
        "issue #574 hidden buffer lines"
    );

    support::eval_expression(&mut gnu, &mut neo, "(setq display-line-numbers 'relative)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let relative = ["4 line 1", "3 [hidden] line 4", "5 line 5"];
    assert_eq!(rows(gnu.text_grid()), relative, "GNU relative-mode oracle");
    assert_eq!(
        rows(neo.text_grid()),
        relative,
        "relative hidden buffer lines"
    );
}

#[test]
fn global_display_line_numbers_lights_the_gutter_in_a_buffer_created_after_startup() {
    let visited = TuiTempFile::new(
        "neomacs-issue-441-visited-",
        "visited.txt",
        format!("{BODY}\nmiddle line\n{TAIL}\n"),
    );
    let init = init_fixture(visited.path());
    let (mut gnu, mut neo) = boot_pair(&format!("-l {}", init.display()));
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // The visited file's first row must carry the absolute line number 1 in
    // the gutter, before the text — GNU's right-aligned number column plus
    // one space.
    let gutter = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains('1') && row.contains(BODY) && row.find('1') < row.find(BODY))
    };
    let gnu_ok = gutter(&gnu.text_grid());
    let neo_ok = gutter(&neo.text_grid());
    if !gnu_ok || !neo_ok {
        dump_pair_grids("global_display_line_numbers_gutter", &gnu, &neo);
    }
    assert!(
        gnu_ok,
        "GNU oracle: the visited file's first line must show gutter number 1"
    );
    assert!(
        neo_ok,
        "issue #441: the file buffer created after the global mode enabled \
         must show line numbers"
    );

    // Fall through to the exact-pair match: it pins the full rendered grid —
    // gutters on every line, the mode line, everything.
    support::assert_pair_exact_display("global_display_line_numbers_lights_the_gutter", &gnu, &neo);
}

/// Diagnostic reproduction: collect the complete matrix before reporting parity
/// failures. Numeric widths and painted prefixes are independent observations.
#[test]
fn narrowed_gutter_width_and_painted_prefix_match_gnu_matrix() {
    use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};
    use std::{fs, path::Path, time::SystemTime};

    struct Observation {
        widths: Option<Vec<f64>>,
        raw_widths: Vec<String>,
        prefixes: Vec<String>,
        tag_columns: Vec<usize>,
        report: serde_json::Value,
    }

    fn capture(
        session: &mut TuiSession,
        case: &str,
        editor: &str,
        enabled: bool,
        output: &TuiTempFile,
        artifacts: &Path,
        failures: &mut Vec<String>,
    ) -> Observation {
        let query = fs::read_to_string(output.path()).unwrap_or_default();
        fs::write(artifacts.join(format!("{case}.{editor}.query.txt")), &query)
            .expect("save gutter query");
        // The first three lines are the independently queried digit columns,
        // canonical columns including padding, and terminal-derived pixels.
        let raw_widths = query.lines().take(3).map(str::to_owned).collect::<Vec<_>>();
        let widths = query
            .lines()
            .take(3)
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()
            .filter(|values| values.len() == 3 && values.iter().all(|n| n.is_finite()));
        match &widths {
            Some(values)
                if values
                    .iter()
                    .all(|n| if enabled { *n > 0.0 } else { *n == 0.0 }) => {}
            _ => failures.push(format!(
                "{case}/{editor}: invalid enabled={enabled} widths {widths:?}"
            )),
        }
        if !query.lines().nth(3).is_some_and(|line| line == "t") {
            failures.push(format!(
                "{case}/{editor}: fixture was not the displayed live window buffer"
            ));
        }
        if !query.lines().nth(4).is_some_and(|line| line == "t") {
            failures.push(format!(
                "{case}/{editor}: queried buffer, local mode, widening or restriction did not match the case"
            ));
        }
        let (grid, raw) = match session.observe_at_fence() {
            Ok(view) => (
                view.text_grid(),
                Some(RawTerminalSnapshot::capture_full_screen(view.screen())),
            ),
            Err(error) => {
                failures.push(format!(
                    "{case}/{editor}: terminal observation failed: {error}"
                ));
                (Vec::new(), None)
            }
        };
        fs::write(
            artifacts.join(format!("{case}.{editor}.grid.txt")),
            grid.join("\n"),
        )
        .expect("save untrimmed gutter grid");
        if let Some(raw) = raw {
            fs::write(
                artifacts.join(format!("{case}.{editor}.grid.ansi")),
                raw.ansi_grid(),
            )
            .expect("save gutter cells with attributes");
        }
        let mut prefixes = Vec::new();
        let mut tag_columns = Vec::new();
        let mut rows = Vec::new();
        for number in 1001..=1005 {
            let tag = format!("ROW{number:04} body {case}");
            let matches = grid
                .iter()
                .enumerate()
                .filter_map(|(row, text)| text.find(&tag).map(|column| (row, column, text)))
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                failures.push(format!(
                    "{case}/{editor}: expected one painted {tag}, found {}",
                    matches.len()
                ));
                continue;
            }
            let (row, column, text) = matches[0];
            let prefix = &text[..column];
            let has_number = prefix.bytes().any(|byte| byte.is_ascii_digit());
            if has_number != enabled
                || !prefix
                    .bytes()
                    .all(|byte| byte == b' ' || byte.is_ascii_digit())
            {
                failures.push(format!(
                    "{case}/{editor}: invalid painted prefix {prefix:?} for {tag}"
                ));
            }
            prefixes.push(prefix.to_owned());
            tag_columns.push(column); // ASCII fixture: byte and terminal-cell columns agree.
            rows.push(serde_json::json!({"tag": tag, "row": row, "column": column, "prefix": prefix, "text": text}));
        }
        let report = serde_json::json!({"editor": editor, "widths": widths, "raw_widths": raw_widths, "query": query, "rows": rows,
            "recording": session.recording_path().map(|path| path.display().to_string())});
        Observation {
            widths,
            raw_widths,
            prefixes,
            tag_columns,
            report,
        }
    }

    // Preserve the existing attested GNU resolver and require Neo's matching
    // dumped runtime, rather than silently measuring source bootstrap.
    neomacs_tui_tests::assert_neomacs_dumped_runtime(&neomacs_tui_tests::neomacs_binary());
    let base = std::env::var_os("NEOMACS_GUTTER_REPRO_ARTIFACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| neomacs_test_env::bounded_temp_base().join("neomacs-gutter"));
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let artifacts = base.join(format!("run-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&artifacts).expect("create gutter reproduction artifacts");
    eprintln!("gutter reproduction artifacts: {}", artifacts.display());
    fs::write(
        artifacts.join("gnu-oracle.txt"),
        neomacs_tui_tests::gnu_oracle_stamp(),
    )
    .expect("save attested GNU identity");

    let (mut gnu, mut neo) = boot_pair("");
    support::resize_both(&mut gnu, &mut neo, 24, 80);
    let mut failures = Vec::new();
    let mut reports = Vec::new();
    let cases = [
        ("absolute-narrow", "t", "nil", false),
        ("absolute-wide", "t", "t", false),
        ("relative-narrow", "'relative", "nil", false),
        ("relative-wide", "'relative", "t", false),
        ("visual-narrow", "'visual", "nil", false),
        ("visual-wide", "'visual", "t", false),
        ("disabled", "nil", "nil", false),
        ("small-positive", "t", "nil", true),
    ];
    for (case, mode, widen, small) in cases {
        let before = failures.len();
        let enabled = mode != "nil";
        let count = if small { 5 } else { 1200 };
        let first = if small { 1001 } else { 1 };
        // ASCII rows have a constant width within a case. Derive the intended
        // restriction independently of the editor's reported bounds.
        let row_stride = format!("ROW1001 body {case}").len() + 1;
        let expected_min = if small { 1 } else { 1000 * row_stride + 1 };
        let expected_max = if small {
            5 * row_stride
        } else {
            1005 * row_stride + 1
        };
        let expected_point = expected_min + 2 * row_stride;
        let setup = format!(
            r#"(progn
          (delete-other-windows)
          (switch-to-buffer (get-buffer-create "*gutter-{case}*"))
          (fundamental-mode)
          (widen) (erase-buffer)
          (dotimes (i {count})
            (insert (format "ROW%04d body {case}" (+ {first} i)))
            (unless (= i (1- {count})) (insert "\n")))
          (setq-local mode-line-format nil header-line-format nil tab-line-format nil
            truncate-lines t word-wrap nil line-prefix nil wrap-prefix nil
            display-line-numbers {mode} display-line-numbers-widen {widen}
            display-line-numbers-offset 0 display-line-numbers-current-absolute t
            display-line-numbers-width nil display-line-numbers-width-start nil
            display-line-numbers-grow-only nil)
          (goto-char (point-min))
          (unless {small_lisp}
            (forward-line 1000)
            (let ((start (point))) (forward-line 5) (narrow-to-region start (point))))
          (goto-char (point-min)) (forward-line 2)
          (set-window-start (selected-window) (point-min) t)
          (set-window-point (selected-window) (point))
          (set-window-hscroll (selected-window) 0)
          (set-window-vscroll (selected-window) 0)
          (redisplay t) nil)"#,
            small_lisp = if small { "t" } else { "nil" }
        );
        fs::write(artifacts.join(format!("{case}.setup.el")), &setup).expect("save gutter fixture");
        support::eval_expression_one(&mut gnu, &setup);
        support::eval_expression_one(&mut neo, &setup);
        read_both(&mut gnu, &mut neo, Duration::from_secs(2));
        // Establish painted source rows before querying the API. A later
        // function result cannot stand in for this presentation observation.
        let mut before_observations = Vec::new();
        let mut before_reports = Vec::new();
        for (editor, session) in [("gnu", &mut gnu), ("neo", &mut neo)] {
            match session.observe_at_fence() {
                Ok(view) => {
                    let grid = view.text_grid();
                    fs::write(
                        artifacts.join(format!("{case}.{editor}.before-query.grid.txt")),
                        grid.join("\n"),
                    )
                    .expect("save presentation before width query");
                    let mut prefixes = Vec::new();
                    let mut columns = Vec::new();
                    let mut rows = Vec::new();
                    for number in 1001..=1005 {
                        let tag = format!("ROW{number:04} body {case}");
                        let matches = grid
                            .iter()
                            .enumerate()
                            .filter_map(|(row, text)| {
                                text.find(&tag).map(|column| (row, column, text))
                            })
                            .collect::<Vec<_>>();
                        if matches.len() != 1 {
                            failures.push(format!(
                                "{case}/{editor}: {tag} was not uniquely painted before query"
                            ));
                            continue;
                        }
                        let (row, column, text) = matches[0];
                        let prefix = &text[..column];
                        let has_number = prefix.bytes().any(|byte| byte.is_ascii_digit());
                        if has_number != enabled
                            || !prefix
                                .bytes()
                                .all(|byte| byte == b' ' || byte.is_ascii_digit())
                        {
                            failures.push(format!(
                                "{case}/{editor}: invalid before-query painted prefix {prefix:?} for {tag}"
                            ));
                        }
                        prefixes.push(prefix.to_owned());
                        columns.push(column);
                        rows.push(serde_json::json!({"tag": tag, "row": row, "column": column,
                            "prefix": prefix, "text": text}));
                    }
                    before_observations.push((prefixes, columns));
                    before_reports.push(serde_json::json!({"editor": editor, "rows": rows}));
                }
                Err(error) => {
                    failures.push(format!(
                        "{case}/{editor}: presentation observation failed: {error}"
                    ));
                    before_reports
                        .push(serde_json::json!({"editor": editor, "error": error.to_string()}));
                }
            }
        }
        let before_prefixes_equal = if let [gnu_before, neo_before] = before_observations.as_slice()
        {
            if gnu_before != neo_before {
                failures.push(format!(
                    "{case}: before-query painted GNU {:?} != Neo {:?}",
                    gnu_before, neo_before
                ));
            }
            Some(gnu_before == neo_before)
        } else {
            None
        };

        let gnu_output = TuiTempFile::new("gutter-gnu-", "query.txt", "");
        let neo_output = TuiTempFile::new("gutter-neo-", "query.txt", "");
        for (editor, session, output) in [
            ("gnu", &mut gnu, &gnu_output),
            ("neo", &mut neo, &neo_output),
        ] {
            let path = serde_json::to_string(&output.path().display().to_string())
                .expect("quote query path");
            let probe = format!(
                r#"(let ((result
              (format "%S\n%S\n%S\n%S\n%S\n%S\n"
                (line-number-display-width) (line-number-display-width 'columns)
                (line-number-display-width t)
                (and (frame-live-p (selected-frame)) (window-live-p (selected-window))
                     (eq (window-buffer (selected-window)) (current-buffer))
                     (= (window-start) (point-min)))
                (and (equal (buffer-name) "*gutter-{case}*")
                     (local-variable-p 'display-line-numbers)
                     (local-variable-p 'display-line-numbers-widen)
                     (eq display-line-numbers {mode})
                     (eq display-line-numbers-widen {widen})
                     (= (point-min) {expected_min}) (= (point-max) {expected_max})
                     (= (point) {expected_point}) (= (window-start) {expected_min})
                     (eq (window-buffer (selected-window)) (current-buffer)))
                (list :case '{case} :mode display-line-numbers :widen display-line-numbers-widen
                  :point-min (point-min) :point-max (point-max) :point (point)
                  :window-start (window-start) :window-end (window-end nil t)
                  :buffer-size (buffer-size) :frame (list (frame-width) (frame-height))
                  :body-height (window-body-height) :frame-char-width (frame-char-width)))))
              (with-temp-file {path} (insert result)) (redisplay t) nil)"#
            );
            fs::write(artifacts.join(format!("{case}.{editor}.probe.el")), &probe)
                .expect("save gutter probe");
            support::eval_expression_one(session, &probe);
        }
        read_both(&mut gnu, &mut neo, Duration::from_secs(2));
        let a = capture(
            &mut gnu,
            case,
            "gnu",
            enabled,
            &gnu_output,
            &artifacts,
            &mut failures,
        );
        let b = capture(
            &mut neo,
            case,
            "neo",
            enabled,
            &neo_output,
            &artifacts,
            &mut failures,
        );
        if a.raw_widths != b.raw_widths {
            failures.push(format!(
                "{case}: typed getter results GNU {:?} != Neo {:?}",
                a.raw_widths, b.raw_widths
            ));
        }
        if a.widths != b.widths {
            failures.push(format!(
                "{case}: queried GNU {:?} != Neo {:?}",
                a.widths, b.widths
            ));
        }
        if a.prefixes != b.prefixes || a.tag_columns != b.tag_columns {
            failures.push(format!(
                "{case}: painted GNU prefixes {:?} at {:?} != Neo {:?} at {:?}",
                a.prefixes, a.tag_columns, b.prefixes, b.tag_columns
            ));
        }
        reports.push(
            serde_json::json!({"case": case, "enabled": enabled, "small": small,
            "before_query": before_reports, "before_query_prefixes_equal": before_prefixes_equal,
            "gnu": a.report, "neo": b.report,
            "failures": &failures[before..]}),
        );
        fs::write(
            artifacts.join("matrix.json"),
            serde_json::to_vec_pretty(&reports).expect("serialize gutter matrix"),
        )
        .expect("save every completed gutter case");
    }
    fs::write(artifacts.join("failures.txt"), failures.join("\n")).expect("save gutter mismatches");
    assert!(
        failures.is_empty(),
        "gutter matrix mismatches (all cases recorded in {}):\n{}",
        artifacts.display(),
        failures.join("\n")
    );
}
