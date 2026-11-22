use super::*;
use std::time::Duration;

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
