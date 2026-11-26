#![cfg(unix)]
//! Issue #640: `long_line_optimizations_p` rows against live GNU.
//!
//! GNU sets the flag during redisplay (`src/xdisp.c:20646-20663`); it is then
//! Lisp-observable through `(long-line-optimizations-p)` (`xdisp.c:20387`),
//! through the accessible region `fontification-functions` sees
//! (`xdisp.c:4702-4719`), and through the region `pre-command-hook` sees
//! (`src/keyboard.c:1983-2001`). Batch rows cannot show the flag, because
//! only redisplay sets it.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

/// One long-line buffer, point at its beginning, truncate-lines on, and a
/// forced redisplay so GNU's flag is set.
const LONG_BUFFER_SETUP: &str = "(progn \
  (switch-to-buffer (get-buffer-create \"*p640-long*\")) \
  (erase-buffer) \
  (insert (make-string 1200000 ?a)) \
  (setq truncate-lines t) \
  (goto-char (point-min)) \
  (redisplay t) \
  nil)";

/// Read the text between `TAGBEGIN` and `TAGEND` in the rendered grid.
fn read_marked(session: &TuiSession, tag: &str) -> Option<String> {
    let begin = format!("{tag}BEGIN");
    let end = format!("{tag}END");
    let (rows, _) = session.screen_size();
    for row in 0..rows {
        let text = session.row_text(row);
        if let Some(start) = text.find(&begin) {
            let rest = &text[start + begin.len()..];
            if let Some(finish) = rest.find(&end) {
                return Some(rest[..finish].to_string());
            }
        }
    }
    None
}

/// Render `EXPR` through a uniquely tagged `message` in both editors and
/// return the two printed values.
fn paired_message(
    gnu: &mut TuiSession,
    neo: &mut TuiSession,
    tag: &str,
    expr: &str,
) -> (String, String) {
    eval_expression(
        gnu,
        neo,
        &format!("(message \"{tag}BEGIN%S{tag}END\" {expr})"),
    );
    read_both(gnu, neo, Duration::from_millis(700));
    let gnu_text = read_marked(gnu, tag).unwrap_or_else(|| {
        panic!(
            "GNU never rendered the {tag} message; grid:\n{}",
            gnu.text_grid().join("\n")
        )
    });
    let neo_text = read_marked(neo, tag).unwrap_or_else(|| {
        panic!(
            "Neomacs never rendered the {tag} message; grid:\n{}",
            neo.text_grid().join("\n")
        )
    });
    (gnu_text, neo_text)
}

fn boot_long_line_pair() -> (TuiSession, TuiSession) {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(&mut gnu, &mut neo, LONG_BUFFER_SETUP);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    (gnu, neo)
}

/// 1000 'a', one TAB (which expands to column 1008), then 200000 'a'.
/// With the long-line flag set, GNU `current_column` returns the character
/// distance from the line start and ignores the TAB (`src/indent.c:348-351`).
const LONG_LINE_TAB_SETUP: &str = "(progn \
  (switch-to-buffer (get-buffer-create \"*p640-s2*\")) \
  (erase-buffer) \
  (insert (make-string 1000 ?a)) \
  (insert \"\\t\") \
  (insert (make-string 200000 ?a)) \
  (setq truncate-lines t) \
  (goto-char 150001) \
  (redisplay t) \
  nil)";

mod columns;
mod current_column_1;
mod declining;
mod fontification;
mod partial_width;
mod scroll;
