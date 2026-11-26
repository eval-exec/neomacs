use super::*;
use std::time::Duration;

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

/// GNU `window_scroll_for_long_lines` (`src/window.c:6248-6266`) starts the
/// whole scroll from the WINDOW START when point is fully visible (it sets
/// point to `w->start` before `vmotion`), and only uses
/// `vmotion (PT, -(ht/2))` when point is outside.  With point at line 5 and
/// window start 1 the new start and point are therefore counted from the
/// window start.  Attested GNU (tmux 160x40): `(200032 200342 200522 37)`.
#[test]
fn scroll_up_starts_from_window_start_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (setq long-line-threshold 50000 large-hscroll-threshold 10000) \
           (switch-to-buffer (get-buffer-create \"*p640-s5*\")) \
           (erase-buffer) \
           (insert (make-string 200000 ?a)) \
           (insert \"\\n\") \
           (let ((i 0)) (while (< i 200) (insert (format \"L%03d line\\n\" i)) (setq i (1+ i)))) \
           (setq truncate-lines t) \
           (goto-char 1) (forward-line 4) \
           (set-window-start (selected-window) 1) \
           (redisplay t) \
           nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640S5",
        "(let ((before (point))) \
           (scroll-up) \
           (list before (window-start) (window-point) (window-body-height)))",
    );
    eprintln!("issue#640 scroll from window start: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(200032 200342 200522 37)",
        "GNU counts the whole scroll from the window start"
    );
    assert_eq!(neo_value, gnu_value, "whole scroll-up must match GNU");
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
           (message (concat \"P640S3F\" \"BEGIN%S\" \"P640S3F\" \"END\") \
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
