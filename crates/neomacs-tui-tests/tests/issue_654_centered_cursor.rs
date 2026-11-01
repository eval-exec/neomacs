//! Issue #654: the "centered cursor" configuration
//!
//! ```elisp
//! (setopt scroll-conservatively 1 scroll-margin 999 maximum-scroll-margin 0.5)
//! ```
//!
//! GNU caps `scroll-margin` through `window_scroll_margin` (src/window.c:6207):
//! the effective margin is `min ((window-lines - 1) / 2, window-lines * ratio)`
//! with the float `maximum-scroll-margin` (0.25 by default, clamped to
//! 0.0..0.5). `try_scrolling` then scrolls exactly one line whenever point
//! moves past the bottom margin, so walking down a buffer one line at a time
//! moves the window one line at a time and point keeps its middle row.
//!
//! The oracle records, after each `forward-line`, the screen row point
//! occupies (`count-lines` between `window-start` and `window-point`, exactly
//! GNU's manual `scroll-tests--scroll-margin-whole-window` measurement) and
//! the window's text height.

use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, Instant};

use neomacs_tui_tests::TuiSession;
use neomacs_tui_tests::pair::{boot_pair, eval_expression_one};

fn centered_cursor_samples() -> (String, String) {
    let (mut gnu, mut neo) = boot_pair("");
    let dir = tempfile::tempdir().expect("temporary directory");
    let gnu_out = dir.path().join("issue-654-gnu.out");
    let neo_out = dir.path().join("issue-654-neomacs.out");
    // The samples are staged under another name and published by rename, so
    // the file exists only once it is complete.
    let form = |path: &Path| {
        let staging = path.with_extension("part");
        format!(
            r##"(let ((buffer (get-buffer-create "*issue-654-center*")))
  (unwind-protect
      (progn
        (switch-to-buffer buffer)
        (erase-buffer)
        (dotimes (i 200) (insert (format "line %d\n" i)))
        (goto-char (point-min))
        (setq scroll-conservatively 1
              scroll-margin 999
              maximum-scroll-margin 0.5)
        (set-window-start nil (point-min) t)
        (sit-for 0)
        (let ((height (window-text-height))
              samples)
          (dotimes (_ 60)
            (forward-line 1)
            (sit-for 0)
            (push (count-lines (window-start) (window-point)) samples))
          (with-temp-file {staging:?}
            (prin1 (list height (nreverse samples)) (current-buffer)))
          (rename-file {staging:?} {path:?} t)))
    (kill-buffer buffer)))"##,
            staging = staging,
            path = path
        )
    };
    eval_expression_one(&mut gnu, &form(&gnu_out));
    eval_expression_one(&mut neo, &form(&neo_out));
    (
        published_samples(&mut gnu, &gnu_out, "GNU"),
        published_samples(&mut neo, &neo_out, "Neomacs"),
    )
}

/// Read EDITOR's sample file once the form has published it.
///
/// `eval_expression_one` returns when the `M-:` prompt leaves the screen,
/// which happens at the form's first `sit-for` redisplay, while the sixty
/// motions are still running; the published file is the completion signal.
/// Any other error, and the deadline, fail with the editor's screen so a form
/// that signalled is told apart from one that was slow.
fn published_samples(session: &mut TuiSession, path: &Path, editor: &str) -> String {
    const DEADLINE: Duration = Duration::from_secs(60);
    const POLL: Duration = Duration::from_millis(100);
    let deadline = Instant::now() + DEADLINE;
    loop {
        match std::fs::read_to_string(path) {
            Ok(text) => return text,
            Err(error) if error.kind() == ErrorKind::NotFound && Instant::now() < deadline => {
                // Keep draining the editor's output while waiting.
                session.read_until(POLL, |_| false);
            }
            Err(error) => panic!(
                "{editor} centered-cursor samples at {}: {error}\n{}",
                path.display(),
                session.text_grid().join("\n")
            ),
        }
    }
}

/// The screen row GNU keeps point on once the window starts following it.
fn steady_row(samples: &[i64]) -> i64 {
    let row = *samples.last().expect("every motion must be sampled");
    let first_scroll = samples
        .iter()
        .position(|sample| *sample == row)
        .expect("the steady row must occur in the samples");
    assert!(
        first_scroll > 0 && samples[first_scroll..].iter().all(|sample| *sample == row),
        "point must keep one screen row while moving one line at a time: {samples:?}"
    );
    row
}

#[test]
fn centered_cursor_keeps_point_on_gnu_s_scroll_margin_row() {
    let (gnu, neo) = centered_cursor_samples();

    // `(height (rows...))`: the first element is window-text-height, the rest
    // are the per-motion point rows.
    let parse = |text: &str| -> (i64, Vec<i64>) {
        let text = text.trim();
        let inner = text
            .strip_prefix('(')
            .and_then(|rest| rest.strip_suffix(')'))
            .expect("(height rows...)");
        let (height, rows) = inner.split_once(' ').expect("height and rows");
        let rows = rows
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split_whitespace()
            .map(|row| row.parse::<i64>().expect("integer row"))
            .collect();
        (height.parse::<i64>().expect("height"), rows)
    };
    let (gnu_height, gnu_rows) = parse(&gnu);
    let (neo_height, neo_rows) = parse(&neo);

    // GNU's margin, recomputed from its own reported text height
    // (window_scroll_margin, src/window.c:6207).
    let gnu_margin = ((gnu_height - 1) / 2).min((gnu_height as f64 * 0.5) as i64);
    let expected_row = gnu_height - 1 - gnu_margin;
    assert_eq!(
        steady_row(&gnu_rows),
        expected_row,
        "GNU oracle must sit on its scroll-margin row: height={gnu_height} rows={gnu_rows:?}"
    );
    assert_eq!(
        neo_height, gnu_height,
        "both editors run in the same PTY, so their text heights must agree"
    );
    assert_eq!(
        steady_row(&neo_rows),
        expected_row,
        "Neomacs must keep point centered like GNU; GNU rows={gnu_rows:?} \
         Neomacs rows={neo_rows:?}"
    );
    assert_eq!(neo_rows, gnu_rows, "per-motion rows must match GNU exactly");
}
