use super::*;
use std::time::Duration;

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
