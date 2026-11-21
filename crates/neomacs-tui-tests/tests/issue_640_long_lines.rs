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

#[test]
fn long_line_optimizations_p_after_redisplay_matches_gnu() {
    let (mut gnu, mut neo) = boot_long_line_pair();
    let (gnu_value, neo_value) =
        paired_message(&mut gnu, &mut neo, "P640LL", "(long-line-optimizations-p)");
    eprintln!("issue#640 predicate: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "t",
        "GNU must report the long-line optimizations"
    );
    assert_eq!(
        neo_value, gnu_value,
        "Neo's (long-line-optimizations-p) must match GNU after redisplay"
    );
}

#[test]
fn fontification_functions_accessible_bounds_match_gnu() {
    let (mut gnu, mut neo) = boot_long_line_pair();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (defvar p640-fmin nil) \
           (defvar p640-fmax nil) \
           (setq p640-fmin nil p640-fmax nil) \
           (set (make-local-variable 'fontification-functions) \
                (list (lambda (pos) \
                        (unless p640-fmin \
                          (setq p640-fmin (point-min) p640-fmax (point-max))) \
                        (put-text-property pos (min (point-max) (+ pos 100000)) \
                                           'fontified t) \
                        nil))) \
           (goto-char (point-min)) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) =
        paired_message(&mut gnu, &mut neo, "P640FB", "(list p640-fmin p640-fmax)");
    eprintln!("issue#640 fontification bounds: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        neo_value, gnu_value,
        "fontification-functions must see GNU's narrowed accessible region"
    );
}

/// GNU's fontification narrowing is centred on the redisplayed window's
/// point, not on the fontified position (`reseat`, `src/xdisp.c:8008-8011`;
/// `handle_fontified_prop`, `src/xdisp.c:4702-4715`). With point at 400000
/// and the window starting at 390001 the point-centred cache is
/// 149872..650000.
///
/// The first fontification call of a buffer can run before the redisplay
/// that sets `long_line_optimizations_p` (`src/xdisp.c:20646-20661`), so it
/// sees the whole buffer.  Record every DISTINCT (point-min, point-max,
/// flag) call for both editors and require GNU's attested narrowed call:
/// (390001 149872 650000 t) from the oracle probe.  Neo must produce the
/// same narrowed bounds; a product difference is a product bug.
#[test]
fn fontification_narrowing_centres_on_window_point() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-point*\")) \
           (erase-buffer) \
           (insert (make-string 1200000 ?a)) \
           (setq truncate-lines t) \
           (defvar p640-ff-calls nil) \
           (setq p640-ff-calls nil) \
           (set (make-local-variable 'fontification-functions) \
                (list (lambda (pos) \
                        (let ((flag (long-line-optimizations-p)) \
                              (seen nil)) \
                          (dolist (entry p640-ff-calls) \
                            (when (and (= (nth 1 entry) (point-min)) \
                                       (= (nth 2 entry) (point-max)) \
                                       (eq (nth 3 entry) flag)) \
                              (setq seen t))) \
                          (unless seen \
                            (setq p640-ff-calls \
                                  (cons (list pos (point-min) (point-max) \
                                              flag (buffer-chars-modified-tick)) \
                                        p640-ff-calls)))) \
                        nil))) \
           (goto-char 400000) \
           (set-window-start nil 390001) \
           (redisplay t) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640WP", "p640-ff-calls");
    eprintln!("issue#640 window-point fontification calls: GNU={gnu_value} NEO={neo_value}");
    // The attested oracle narrowed call.  The chars-modified tick that follows
    // it in the printed entry is host state, so match up to the flag.
    assert!(
        gnu_value.contains("149872 650000 t"),
        "GNU's attested window-point-centred bounds missing: {gnu_value}"
    );
    assert!(
        neo_value.contains("149872 650000 t"),
        "Neo must produce the same narrowed bounds as GNU: {neo_value}"
    );
}

/// GNU's hscroll pass measures the cursor column on a truncated line with
/// `fast_move_it_horizontally` above `large-hscroll-threshold`
/// (`src/xdisp.c:16797-16807`), which advances one frame column per character
/// from the row start. The resulting `window-hscroll` is Lisp-observable, so
/// the approximation itself is part of the contract.
#[test]
fn long_line_hscroll_matches_gnu() {
    let (mut gnu, mut neo) = boot_long_line_pair();
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640HS",
        "(progn (goto-char (point-max)) (redisplay t) (window-hscroll))",
    );
    eprintln!("issue#640 long-line hscroll: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        neo_value, gnu_value,
        "the long-line hscroll approximation must match GNU"
    );
}

#[test]
fn pre_command_hook_accessible_bounds_match_gnu() {
    let (mut gnu, mut neo) = boot_long_line_pair();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (defvar p640-cmin nil) \
           (defvar p640-cmax nil) \
           (setq p640-cmin nil p640-cmax nil) \
           (set (make-local-variable 'pre-command-hook) \
                (list (lambda () \
                        (unless p640-cmin \
                          (setq p640-cmin (point-min) p640-cmax (point-max)))))) \
           (goto-char (point-min)) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    // Any command runs pre-command-hook with point at the beginning of the
    // long line. C-n does not move point in a one-line buffer.
    send_both(&mut gnu, &mut neo, "C-n");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) =
        paired_message(&mut gnu, &mut neo, "P640CH", "(list p640-cmin p640-cmax)");
    eprintln!("issue#640 command-hook bounds: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        neo_value, gnu_value,
        "pre-command-hook must see GNU's narrowed accessible region"
    );
}

// ---------------------------------------------------------------------------
// Slice 2: the remaining Lisp-observable long-line paths.
// The flag is set only by redisplay, so these rows must run in a real TUI
// session; batch GNU can never set long_line_optimizations_p.
// ---------------------------------------------------------------------------

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

#[test]
fn current_column_approximates_long_lines_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(&mut gnu, &mut neo, LONG_LINE_TAB_SETUP);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640CC", "(current-column)");
    eprintln!("issue#640 current-column: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "150000",
        "GNU returns the character distance here"
    );
    assert_eq!(neo_value, gnu_value, "current-column must match GNU");
}

/// GNU's approximation returns before `last_known_column` is stored
/// (`src/indent.c:348-351`), so binding `long-line-threshold` to nil at the
/// same point and MODIFF makes the next call exact.
#[test]
fn current_column_approximation_is_not_cached_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(&mut gnu, &mut neo, LONG_LINE_TAB_SETUP);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640NC",
        "(list (current-column) (let ((long-line-threshold nil)) (current-column)))",
    );
    eprintln!("issue#640 current-column cache: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(150000 150007)",
        "GNU: approximation first, exact after the nil threshold"
    );
    assert_eq!(neo_value, gnu_value, "the approximation must not be cached");
}

/// Control: without the flag (line below the threshold) the TAB is counted.
#[test]
fn current_column_below_the_long_line_threshold_is_exact() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-s2-short*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) \
           (insert \"\\t\") \
           (insert (make-string 40000 ?a)) \
           (setq truncate-lines t) \
           (goto-char 30001) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640SC", "(current-column)");
    eprintln!("issue#640 current-column below threshold: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(gnu_value, "30007", "GNU counts the TAB below the threshold");
    assert_eq!(neo_value, gnu_value, "below the threshold both are exact");
}

/// Control: `long-line-threshold` nil disables the approximation, so the flag
/// is never set and the TAB is counted even on a 200k line.
#[test]
fn current_column_with_nil_threshold_is_exact_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold nil) \
           (switch-to-buffer (get-buffer-create \"*p640-s2-nil*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) \
           (insert \"\\t\") \
           (insert (make-string 200000 ?a)) \
           (setq truncate-lines t) \
           (goto-char 150001) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640NT", "(current-column)");
    eprintln!("issue#640 current-column nil threshold: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "150007",
        "nil threshold disables the approximation"
    );
    assert_eq!(neo_value, gnu_value, "both must count the TAB here");
}

/// Slice 2, site 2: GNU `scan_for_column`'s long+truncated shortcut
/// (`src/indent.c:629-668`) makes `move-to-column` one column per character
/// and ignores the TAB.  With truncate-lines t the fast path returns GNU's
/// goal and leaves point at `BOL + goal`.
#[test]
fn move_to_column_long_truncated_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-s2-mtc*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) \
           (insert \"\\t\") \
           (insert (make-string 200000 ?a)) \
           (insert \"\\n\") \
           (setq truncate-lines t) \
           (goto-char 1) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640MT",
        "(list (move-to-column 150000) (point))",
    );
    eprintln!("issue#640 move-to-column: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(150000 150001)",
        "GNU's truncated shortcut ignores the TAB and seats point at BOL+goal"
    );
    assert_eq!(neo_value, gnu_value, "move-to-column must match GNU");
}

/// `current-column` reaches the scan only through `current_column_1`
/// (intervals present, `src/indent.c:365-370`); with `long-line-threshold`
/// nil the :348 approximation is off, so the fast path at :629 is what makes
/// the two buffers differ.  A property leaves GNU's interval tree in place
/// even after the property is removed, so the control uses a second buffer
/// that never had one.
#[test]
fn current_column_1_long_truncated_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (let ((prop (get-buffer-create \"*p640-c1-prop*\")) \
                 (plain (get-buffer-create \"*p640-c1-plain*\"))) \
             (dolist (b (list prop plain)) \
               (set-buffer b) (erase-buffer) \
               (insert (make-string 1000 ?a)) (insert \"\\t\") \
               (insert (make-string 200000 ?a)) (insert \"\\n\") \
               (setq truncate-lines t) (goto-char 30001) \
               (switch-to-buffer b) (redisplay t)) \
             (set-buffer prop) (put-text-property 1 2 'p640 t) \
             (setq long-line-threshold nil) \
             (switch-to-buffer plain)) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640C1",
        "(list (with-current-buffer \"*p640-c1-prop*\" (current-column)) \
                (with-current-buffer \"*p640-c1-plain*\" (current-column)))",
    );
    eprintln!("issue#640 current_column_1: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(30000 30007)",
        "GNU: properties branch approximates, the property-free buffer does not"
    );
    assert_eq!(neo_value, gnu_value, "current_column_1 must match GNU");
}

/// GNU's interval tree is tree PRESENCE, not "has a property":
/// `remove-text-properties` validates with `soft` and never discards the
/// root (`src/textprop.c:1607-1619`), so after the last property is removed
/// `buffer_intervals` is still non-NULL and `current-column` still takes
/// `current_column_1` (`src/indent.c:365-370`) and its long+truncated
/// shortcut.  Attested: GNU prints 30000.
#[test]
fn current_column_1_removed_property_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (switch-to-buffer (get-buffer-create \"*p640-c1-removed*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) (insert \"\\t\") \
           (insert (make-string 200000 ?a)) (insert \"\\n\") \
           (setq truncate-lines t) (goto-char 30001) \
           (redisplay t) \
           (put-text-property 1 2 'p640 t) \
           (remove-text-properties 1 2 '(p640 nil)) \
           (setq long-line-threshold nil) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640C1R", "(current-column)");
    eprintln!("issue#640 removed-property current_column_1: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "30000",
        "GNU keeps the interval tree after remove-text-properties"
    );
    assert_eq!(
        neo_value, gnu_value,
        "the removed-property row must match GNU"
    );
}

/// Partial-width rule (`src/indent.c:637-643`): a 40-column window with
/// truncate-lines nil and `truncate-partial-width-windows` 50 truncates, so
/// the fast path applies.
#[test]
fn move_to_column_partial_width_window_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-s2-pw*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) \
           (insert \"\\t\") \
           (insert (make-string 200000 ?a)) \
           (insert \"\\n\") \
           (setq truncate-lines nil) \
           (setq truncate-partial-width-windows 50) \
           (split-window (selected-window) 40 'right) \
           (set-window-buffer (selected-window) (current-buffer)) \
           (set-buffer (current-buffer)) \
           (goto-char 1) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640PW",
        "(list (move-to-column 150000) (point))",
    );
    eprintln!("issue#640 partial-width move-to-column: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(150000 150001)",
        "a partial-width window truncates, so GNU takes the shortcut"
    );
    assert_eq!(
        neo_value, gnu_value,
        "the partial-width rule must match GNU"
    );
}

/// R1: `Fget_buffer_window (buffer, nil)` with nil ALL-FRAMES searches the
/// SELECTED frame only (`src/window.c:3314-3326`).  A buffer displayed only
/// on another frame therefore gets no partial-width truncation: with
/// truncate-lines nil GNU takes the exact walk, even though that other
/// frame's window is narrow.  The current buffer must be set inside the
/// same command (the selected frame keeps showing another buffer), so this
/// row embeds its message.
#[test]
fn move_to_column_other_frame_window_stays_exact_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (let* ((orig (selected-frame)) \
                  (buf (get-buffer-create \"*p640-s2-of*\")) \
                  (other (make-frame))) \
             (select-frame other) \
             (set-window-buffer (selected-window) buf) \
             (set-buffer buf) \
             (erase-buffer) \
             (insert (make-string 1000 ?a)) (insert \"\\t\") \
             (insert (make-string 200000 ?a)) (insert \"\\n\") \
             (setq truncate-lines nil) \
             (setq truncate-partial-width-windows 50) \
             (split-window (selected-window) 40 'right) \
             (set-window-buffer (selected-window) buf) \
             (goto-char 1) \
             (redisplay t) \
             (select-frame orig) \
             (set-buffer buf) \
             (goto-char 1) \
             (message (concat \"P640OF\" \"BEGIN%S \" \"P640OF\" \"END\") \
                      (list (move-to-column 150000) (point)))))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_value = read_marked(&gnu, "P640OF").unwrap_or_else(|| {
        panic!(
            "GNU never rendered the other-frame value; grid:\n{}",
            gnu.text_grid().join("\n")
        )
    });
    let neo_value = read_marked(&neo, "P640OF").unwrap_or_else(|| {
        panic!(
            "Neomacs never rendered the other-frame value; grid:\n{}",
            neo.text_grid().join("\n")
        )
    });
    eprintln!("issue#640 other-frame partial width: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(150000 149994)",
        "no window in the selected frame: GNU walks exactly (point 149994)"
    );
    assert_eq!(
        neo_value, gnu_value,
        "another frame's window must not enable the partial-width shortcut"
    );
}

#[test]
fn scroll_up_long_truncated_line_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-s3*\")) \
           (erase-buffer) \
           (insert (make-string 200000 ?a)) \
           (insert \"\\n\") \
           (let ((i 0)) (while (< i 200) (insert (format \"L%03d line\\n\" i)) (setq i (1+ i)))) \
           (setq truncate-lines t) \
           (setq scroll-preserve-screen-position t) \
           (setq scroll-margin 5) \
           (goto-char 1) \
           (set-window-start (selected-window) 1) \
           (set-window-point (selected-window) 1) \
           (redisplay t) \
           (scroll-up) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640S3",
        "(list (window-start) (window-point))",
    );
    eprintln!("issue#640 long-line scroll-up: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        neo_value, gnu_value,
        "whole scroll-up on a long truncated line must match GNU"
    );
}

/// Numeric ARG is not a whole scroll (`src/window.c:7008-7014`): GNU stays on
/// its ordinary path and must not take the long-lines algorithm.
#[test]
fn scroll_up_numeric_arg_is_not_the_long_line_path() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-s3n*\")) \
           (erase-buffer) \
           (insert (make-string 200000 ?a)) \
           (insert \"\\n\") \
           (let ((i 0)) (while (< i 200) (insert (format \"L%03d line\\n\" i)) (setq i (1+ i)))) \
           (setq truncate-lines t) \
           (goto-char 1) \
           (set-window-start (selected-window) 1) \
           (set-window-point (selected-window) 1) \
           (redisplay t) \
           (scroll-up 5) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640S3N",
        "(list (window-start) (window-point))",
    );
    eprintln!("issue#640 numeric scroll-up: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(neo_value, gnu_value, "numeric scroll-up must match GNU");
}

/// N1: GNU binds `fontification-functions` to nil for a whole scroll when
/// long lines are truncated (`src/window.c:6178-6179`), and Neo's scroll path
/// runs no Lisp either.  The count is read inside the same command: the
/// post-command redisplay can legitimately fontify after the scroll returns
/// (GNU's attested count then becomes nonzero), so only the in-command value
/// pins the scroll path.
#[test]
fn whole_scroll_runs_no_fontification() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-s3f*\")) \
           (erase-buffer) \
           (insert (make-string 200000 ?a)) \
           (insert \"\\n\") \
           (let ((i 0)) (while (< i 200) (insert (format \"L%03d line\\n\" i)) (setq i (1+ i)))) \
           (setq truncate-lines t) \
           (defvar p640-s3-calls 0) \
           (setq p640-s3-calls 0) \
           (set (make-local-variable 'fontification-functions) \
                (list (lambda (_pos) (setq p640-s3-calls (1+ p640-s3-calls)) nil))) \
           (goto-char 1) \
           (set-window-start (selected-window) 1) \
           (set-window-point (selected-window) 1) \
           (redisplay t) \
           (setq p640-s3-calls 0) \
           (scroll-up) \
           (message (concat \"P640S3F\" \"BEGIN%S \" \"P640S3F\" \"END\") \
                    p640-s3-calls))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gnu_value = read_marked(&gnu, "P640S3F").unwrap_or_else(|| {
        panic!(
            "GNU never rendered the scroll count; grid:\n{}",
            gnu.text_grid().join("\n")
        )
    });
    let neo_value = read_marked(&neo, "P640S3F").unwrap_or_else(|| {
        panic!(
            "Neomacs never rendered the scroll count; grid:\n{}",
            neo.text_grid().join("\n")
        )
    });
    eprintln!("issue#640 scroll fontification calls: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "0",
        "GNU runs no fontification inside the scroll"
    );
    assert_eq!(
        neo_value, gnu_value,
        "Neo's scroll path must not run fontification Lisp"
    );
}

/// Site 4: GNU `Frecenter` uses line-based vmotion when the buffer's
/// long-line flag is set (`src/window.c:7293-7296`, :7400-7434), ignoring
/// pixel line heights; point does not move.  The parent Neo path runs the
/// display iterator.
#[test]
fn recenter_long_line_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-s4*\")) \
           (erase-buffer) \
           (insert (make-string 200000 ?a)) \
           (insert \"\\n\") \
           (let ((i 0)) (while (< i 400) (insert (format \"L%03d line\\n\" i)) (setq i (1+ i)))) \
           (setq truncate-lines t) \
           (goto-char 1) \
           (forward-line 299) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640S4",
        "(let* ((ht (window-body-height)) (pt (point))) \
           (recenter) \
           (let ((center-ok \
                  (list (= (point) pt) \
                        (save-excursion (goto-char pt) (forward-line (- (/ ht 2))) \
                                        (= (window-start) (point)))))) \
             (recenter -5) \
             (list center-ok \
                   (= (point) pt) \
                   (save-excursion (goto-char pt) (forward-line (- 5 ht)) \
                                   (= (window-start) (point))))))",
    );
    eprintln!("issue#640 recenter: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "((t t) t t)",
        "GNU's line-based recenter placement at -5 lines from the top"
    );
    assert_eq!(
        neo_value, gnu_value,
        "recenter must match GNU on a flag buffer"
    );
}

/// GNU's `current_column` line start is `find_newline (PT, ..., BEGV)`
/// (`src/indent.c:344-345`): in a narrowed buffer the walk stops at BEGV, so
/// the approximation returns PT - BEGV.  Neo's `bol_emacs_byte` must use the
/// accessible start the same way; using the buffer's first line would return
/// 399999 instead of the attested 299999.
#[test]
fn current_column_stops_at_begv_in_a_narrowed_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (switch-to-buffer (get-buffer-create \"*p640-narrow*\")) \
           (erase-buffer) \
           (insert (make-string 1000 ?a)) (insert \"\\t\") \
           (insert (make-string 1200000 ?a)) \
           (setq truncate-lines t) \
           (narrow-to-region 100001 600000) \
           (goto-char 400000) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, "P640NB", "(current-column)");
    eprintln!("issue#640 narrowed current-column: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "299999",
        "GNU stops the line start at BEGV (400000 - 100001)"
    );
    assert_eq!(neo_value, gnu_value, "BEGV must bound Neo's line start too");
}
