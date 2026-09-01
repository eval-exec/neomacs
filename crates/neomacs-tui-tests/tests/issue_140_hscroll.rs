//! Issue #140: with `truncate-lines` non-nil, `C-e` to the end of a line that
//! extends past the window's right edge must auto-hscroll so the cursor stays
//! visible — matching GNU's `hscroll_window_tree` exactly. Before the fix
//! neomacs left `window-hscroll` at 0 and dropped the cursor off-screen.
//!
//! Two branches of GNU's centering target are exercised (the difference is
//! GNU `ITERATOR_AT_END_OF_LINE_P` = the char AT point is a newline):
//!   * point at EOB, no trailing newline  -> centering, `text_cols/2`  (hscroll 220)
//!   * point before a trailing newline    -> end-of-line, `text_cols-4` (hscroll 144)
//! Both must equal live GNU's `window-hscroll`.
mod support;

use neomacs_tui_tests::TuiSession;
use std::time::Duration;
use support::*;

/// Read `(window-hscroll)` out of one editor via a uniquely-marked message.
fn window_hscroll(s: &mut TuiSession) -> i64 {
    eval_expression_one(s, "(message \"HSXX%dXX\" (window-hscroll))");
    s.read(Duration::from_millis(500));
    let (rows, _) = s.screen_size();
    for r in (0..rows).rev() {
        let t = s.row_text(r);
        if let Some(i) = t.find("HSXX") {
            let n: String = t[i + 4..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(v) = n.parse::<i64>() {
                return v;
            }
        }
    }
    -1
}

/// Run `setup` (which inserts a long line + sets truncate-lines + goes to BOL),
/// press C-e on both editors, and return (gnu_hscroll, neo_hscroll, gnu_cursor,
/// neo_cursor).
fn ctrl_e_scenario(setup: &str) -> (i64, i64, (u16, u16), (u16, u16)) {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(&mut gnu, &mut neo, setup);
    read_both(&mut gnu, &mut neo, Duration::from_millis(900));
    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gc = gnu.screen().cursor_position();
    let nc = neo.screen().cursor_position();
    let gh = window_hscroll(&mut gnu);
    let nh = window_hscroll(&mut neo);
    (gh, nh, gc, nc)
}

fn assert_matches_gnu(label: &str, gh: i64, nh: i64, gc: (u16, u16), nc: (u16, u16)) {
    eprintln!(
        "issue#140 [{label}]: GNU hscroll={gh} cursor={gc:?}  NEO hscroll={nh} cursor={nc:?}"
    );
    assert!(
        gh > 0,
        "[{label}] precondition: GNU must auto-hscroll (got {gh})"
    );
    assert_eq!(nh, gh, "[{label}] neomacs window-hscroll must equal GNU's");
    assert_eq!(
        nc, gc,
        "[{label}] neomacs cursor row and column must exactly equal GNU's",
    );
}

// ── FOLLOW-UP A: manual hscroll must stick (suspend_auto_hscroll) ──────────

/// FOLLOW-UP A: a manual `scroll-left` WITHOUT moving point must NOT be
/// overridden by the auto-hscroll redisplay pass — GNU `set_window_hscroll`
/// (via scroll-left) sets `w->suspend_auto_hscroll`, which `hscroll_window_tree`
/// STEP 5 honors until window point explicitly moves.  Then moving point far
/// right un-suspends (STEP 4) and auto-hscroll fires again.  neomacs must match
/// GNU's `window-hscroll` at every step.
#[test]
fn issue_140_manual_scroll_left_sticks_then_unsuspends() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    // Long truncated line; leave point at BOL so a recompute (if it fired)
    // would reset hscroll to 0.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (erase-buffer) (setq truncate-lines t) \
         (insert (make-string 300 ?x) 10) (goto-char (point-min)) nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(900));

    // Manually scroll left 50 columns WITHOUT moving point, then force a
    // redisplay. With point at column 0 and hscroll 50, point sits in the
    // left margin, so WITHOUT suspend the auto-hscroll pass would pull
    // hscroll back toward 0. suspend must keep it at 50.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (scroll-left 50) (redisplay) nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gh = window_hscroll(&mut gnu);
    let nh = window_hscroll(&mut neo);
    eprintln!("issue#140 [A/manual-stick]: GNU hscroll={gh}  NEO hscroll={nh}");
    assert_eq!(
        gh, 50,
        "[A] precondition: GNU manual scroll-left must stick"
    );
    assert_eq!(
        nh, gh,
        "[A] neomacs manual scroll-left must stick (suspend_auto_hscroll), matching GNU"
    );

    // Now move point far to the right (C-e -> end of the 300-x line). Point
    // explicitly changed, so STEP 4 un-suspends and auto-hscroll fires again
    // on both editors.
    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gh2 = window_hscroll(&mut gnu);
    let nh2 = window_hscroll(&mut neo);
    eprintln!("issue#140 [A/unsuspend]: GNU hscroll={gh2}  NEO hscroll={nh2}");
    assert!(
        gh2 > 50,
        "[A] precondition: GNU must auto-hscroll past the manual 50 after point moves (got {gh2})"
    );
    assert_eq!(
        nh2, gh2,
        "[A] after point moves, neomacs auto-hscroll un-suspends and matches GNU"
    );
}

// ── FOLLOW-UP B: line-number gutter offset ────────────────────────────────

/// FOLLOW-UP B: with `display-line-numbers` on, the gutter consumes columns at
/// the left, so the usable line-text width is `body_cols - gutter_cols`. GNU
/// subtracts `x_offset` (the gutter pixel width) in the hscroll math, which
/// shifts `window-hscroll` versus the no-line-numbers case. neomacs must match
/// GNU's value with the gutter present.
#[test]
fn issue_140_ce_with_line_numbers_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (erase-buffer) (setq truncate-lines t) (setq display-line-numbers t) \
         (insert (make-string 300 ?x) 10) (goto-char (point-min)) nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(900));
    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let gh = window_hscroll(&mut gnu);
    let nh = window_hscroll(&mut neo);
    eprintln!("issue#140 [B/line-numbers]: GNU hscroll={gh}  NEO hscroll={nh}");
    assert!(
        gh > 0,
        "[B] precondition: GNU must auto-hscroll with line numbers on (got {gh})"
    );
    assert_eq!(
        nh, gh,
        "[B] neomacs window-hscroll with the line-number gutter must equal GNU's"
    );
}

#[test]
fn issue_140_ce_at_eob_no_newline_centers() {
    // 300 x's, NO trailing newline -> point at EOB -> GNU centers (text_cols/2).
    let (gh, nh, gc, nc) = ctrl_e_scenario(
        "(progn (erase-buffer) (setq truncate-lines t) \
         (insert (make-string 300 ?x)) (goto-char (point-min)) nil)",
    );
    assert_matches_gnu("EOB/centering", gh, nh, gc, nc);
}

#[test]
fn issue_140_ce_before_newline_targets_eol() {
    // 300 x's + a newline -> point before the newline -> GNU end-of-line (text_cols-4).
    let (gh, nh, gc, nc) = ctrl_e_scenario(
        "(progn (erase-buffer) (setq truncate-lines t) \
         (insert (make-string 300 ?x) 10) (goto-char (point-min)) nil)",
    );
    assert_matches_gnu("before-newline/EOL", gh, nh, gc, nc);
}
