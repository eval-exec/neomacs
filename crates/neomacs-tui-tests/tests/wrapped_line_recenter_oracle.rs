//! `forward-line` through wrapped lines with the default scroll settings.
//!
//! With `scroll-conservatively` and `scroll-step` 0, GNU `redisplay_window`
//! recenters a point that left the window (`recenter:`, src/xdisp.c:21128)
//! by moving back half the window's DISPLAY height from point
//! (`move_it_vertically_backward`, src/xdisp.c:21223). Point lands mid-window,
//! so walking down 520-character lines changes `window-start` once per half
//! window, not on every motion.
//!
//! The oracle counts `window-start` changes over 200 `forward-line` motions,
//! each followed by `redisplay`.

use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, Instant};

use neomacs_tui_tests::TuiSession;
use neomacs_tui_tests::pair::{boot_pair, eval_expression_one};

fn window_start_changes() -> (String, String) {
    let (mut gnu, mut neo) = boot_pair("");
    let dir = tempfile::tempdir().expect("temporary directory");
    let gnu_out = dir.path().join("wrapped-recenter-gnu.out");
    let neo_out = dir.path().join("wrapped-recenter-neomacs.out");
    // The result is staged under another name and published by rename, so
    // the file exists only once it is complete.
    let form = |path: &Path| {
        let staging = path.with_extension("part");
        format!(
            r##"(let ((buffer (get-buffer-create "*wrapped-recenter*")))
  (unwind-protect
      (progn
        (switch-to-buffer buffer)
        (erase-buffer)
        (dotimes (_ 400) (insert (make-string 520 ?x) "\n"))
        (goto-char (point-min))
        (setq-local scroll-conservatively 0)
        (setq-local scroll-step 0)
        (setq-local scroll-margin 0)
        (setq-local truncate-lines nil)
        (set-window-start nil (point-min) t)
        (redisplay t)
        (let ((height (window-text-height))
              (start (window-start))
              (changes 0))
          (dotimes (_ 200)
            (forward-line 1)
            (redisplay t)
            (unless (= (window-start) start)
              (setq changes (1+ changes) start (window-start))))
          (with-temp-file {staging:?}
            (prin1 (list height changes) (current-buffer)))
          (rename-file {staging:?} {path:?} t)))
    (kill-buffer buffer)))"##,
            staging = staging,
            path = path
        )
    };
    eval_expression_one(&mut gnu, &form(&gnu_out));
    eval_expression_one(&mut neo, &form(&neo_out));
    (
        published_result(&mut gnu, &gnu_out, "GNU"),
        published_result(&mut neo, &neo_out, "Neomacs"),
    )
}

/// Read EDITOR's result file once the form has published it.
///
/// `eval_expression_one` returns when the `M-:` prompt leaves the screen,
/// while the motions may still be running; the published file is the
/// completion signal.
fn published_result(session: &mut TuiSession, path: &Path, editor: &str) -> String {
    const DEADLINE: Duration = Duration::from_secs(120);
    const POLL: Duration = Duration::from_millis(100);
    let deadline = Instant::now() + DEADLINE;
    loop {
        match std::fs::read_to_string(path) {
            Ok(text) => return text,
            Err(error) if error.kind() == ErrorKind::NotFound && Instant::now() < deadline => {
                session.read_until(POLL, |_| false);
            }
            Err(error) => panic!(
                "{editor} wrapped-line recenter result at {}: {error}\n{}",
                path.display(),
                session.text_grid().join("\n")
            ),
        }
    }
}

/// `(HEIGHT CHANGES)` as printed by the form.
fn parse(text: &str) -> (i64, i64) {
    let fields: Vec<i64> = text
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split_whitespace()
        .map(|field| field.parse().expect("integer field"))
        .collect();
    match fields.as_slice() {
        [height, changes] => (*height, *changes),
        _ => panic!("expected (HEIGHT CHANGES), got {text:?}"),
    }
}

#[test]
fn forward_line_through_wrapped_lines_changes_window_start_like_gnu() {
    let (gnu, neo) = window_start_changes();
    let (gnu_height, gnu_changes) = parse(&gnu);
    let (neo_height, neo_changes) = parse(&neo);
    assert_eq!(
        neo_height, gnu_height,
        "both editors run in the same PTY, so their text heights must agree"
    );
    assert_eq!(
        neo_changes, gnu_changes,
        "200 forward-line motions through 520-character lines in a \
         {gnu_height}-row window: window-start changes, Neomacs vs GNU"
    );
}
