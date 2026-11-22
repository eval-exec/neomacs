use super::*;
use std::time::Duration;

/// GNU `partial_line_height`'s probe row (`src/xdisp.c:11630-11661`).
///
/// When `try_window` runs with TRY_WINDOW_CHECK_MARGINS, GNU re-walks to the
/// window bottom and then scans the FIRST screen row after the window end
/// with `move_it_by_lines (&it, 1)`; `handle_stop` enters a declining
/// fontification hook at each stop inside that row.  `forced` adds GNU's
/// force_start pass (`set-window-start` without NOFORCE), which must NOT
/// probe, and a NOFORCE t pass, which must.
///
/// Each segment is one `(redisplay t)`; the query reports the final
/// window-end plus, per segment, the distinct in-window stop count and the
/// distinct beyond-window-end stop set.  Raw call counts are not pinned
/// because they depend on how many redisplay passes run.
fn declining_probe_scene(
    tag: &str,
    line_expr: &str,
    truncate_lines: bool,
    forced: bool,
) -> (String, String) {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    let segments = if forced {
        "(redisplay t) (setq probe-segment 1) \
         (set-window-start (selected-window) 1) (redisplay t) \
         (setq probe-segment 2) (set-window-start (selected-window) 1 t) (redisplay t) nil"
    } else {
        "(redisplay t) nil"
    };
    let truncate = if truncate_lines {
        "(setq truncate-lines t)"
    } else {
        ""
    };
    let setup = format!(
        "(progn \
           (switch-to-buffer (get-buffer-create \"*p640-probe*\")) (erase-buffer) \
           (let ((i 0)) \
             (while (< i 60) \
               (insert (propertize {line_expr} 'p640 i)) (insert \"\\n\") \
               (setq i (1+ i)))) \
           {truncate} \
           (goto-char 1) \
           (defvar probe-calls nil) (setq probe-calls nil) \
           (defvar probe-segment 0) (setq probe-segment 0) \
           (set (make-local-variable 'fontification-functions) \
                (list (lambda (pos) \
                        (setq probe-calls (cons (cons probe-segment pos) probe-calls)) \
                        nil))) \
           {segments})",
    );
    eval_expression(&mut gnu, &mut neo, &setup);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let query = if forced {
        "(let* ((u (delete-dups (reverse probe-calls))) \
                (we (window-end nil t))) \
           (let ((seg (lambda (n) \
                        (let ((ps (delete-dups \
                                    (mapcar (function cdr) \
                                            (seq-filter (lambda (c) (= (car c) n)) u))))) \
                          (list (length (seq-remove (lambda (p) (>= p we)) ps)) \
                                (or (seq-filter (lambda (p) (>= p we)) ps) nil)))))) \
             (list we (funcall seg 0) (funcall seg 1) (funcall seg 2))))"
    } else {
        "(let* ((u (delete-dups (reverse probe-calls))) (we (window-end nil t)) \
                (ps (delete-dups (mapcar (function cdr) u)))) \
           (list we (length (seq-remove (lambda (p) (>= p we)) ps)) \
                 (or (seq-filter (lambda (p) (>= p we)) ps) nil)))"
    };
    let (gnu_value, neo_value) = paired_message(&mut gnu, &mut neo, tag, query);
    eprintln!("issue#640 declining probe row {tag}: GNU={gnu_value} NEO={neo_value}");
    (gnu_value, neo_value)
}

/// A fontification function that declines to set `fontified` must not turn
/// one redisplay into a per-character Lisp walk.  GNU's `handle_fontified_prop`
/// returns HANDLED_NORMALLY and the iterator advances to its next
/// `compute_stop_pos` stop (`src/xdisp.c:4371-4461`, :4782-4786): with an
/// interval tree present but no tracked property on the intervals, those stops
/// are the automatic-composition delimiter clamp -- the first newline at or
/// after `POS + 100`.  Attested GNU on this 60-line form: 39 distinct
/// positions, beginning 1 151 302 453 604 755.  The total call count depends
/// on how many redisplay passes run, so only the distinct list is pinned.
/// The summary is one line: the full 39-element list wraps the echo area, and
/// the harness reads the tag from a single row.
#[test]
fn declining_fontification_visits_gnu_stops_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, 40, 160);
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn \
           (switch-to-buffer (get-buffer-create \"*declp*\")) (erase-buffer) \
           (dotimes (i 60) (insert (propertize (make-string 150 ?a) 'p640 i) \"\\n\")) \
           (goto-char 1) (defvar decl-calls nil) (setq decl-calls nil) \
           (set (make-local-variable 'fontification-functions) \
                (list (lambda (pos) (push pos decl-calls) nil))) \
           (redisplay t) nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    let (gnu_value, neo_value) = paired_message(
        &mut gnu,
        &mut neo,
        "P640DECL",
        "(let ((u (delete-dups (reverse decl-calls)))) \
           (list (length u) (seq-take u 6) (apply (function +) u) (car (last u))))",
    );
    eprintln!("issue#640 declining fontification stops: GNU={gnu_value} NEO={neo_value}");
    assert_eq!(
        gnu_value, "(39 (1 151 302 453 604 755) 111892 5738)",
        "GNU's attested distinct stops, position sum, and last stop"
    );
    assert_eq!(
        neo_value, gnu_value,
        "a declining hook must be entered at GNU's stops, not per character"
    );
}

/// DFACE: face stops inside the probe row (line 38) are entered; the next
/// row's stop (5789) is not.
#[test]
fn declining_fontification_probes_face_stops_like_gnu() {
    let (gnu_value, neo_value) = declining_probe_scene(
        "P640FACE",
        "(concat (make-string 50 ?a) \
                 (propertize (make-string 10 ?b) 'face 'bold) \
                 (make-string 90 ?a))",
        false,
        false,
    );
    assert_eq!(
        gnu_value, "(5588 75 (5638 5648))",
        "GNU's DFACE probe-row stops"
    );
    assert_eq!(neo_value, gnu_value, "DFACE probe row must match GNU");
}

/// DLONG: 400-column lines wrap; the probe row has no stop (the next
/// newline, 5213, lies in the following row).
#[test]
fn declining_fontification_probe_stays_in_its_row_for_wrapped_lines() {
    let (gnu_value, neo_value) =
        declining_probe_scene("P640LONG", "(make-string 400 ?a)", false, false);
    assert_eq!(
        gnu_value, "(4972 13 nil)",
        "GNU's DLONG probe row adds no stop"
    );
    assert_eq!(neo_value, gnu_value, "DLONG probe row must match GNU");
}

/// DSHORT: 30-column lines; the last in-window stop is 1116 and the probe
/// row [1148,1179) contains no stop (1240 is in the next row).
#[test]
fn declining_fontification_probe_short_lines_matches_gnu() {
    let (gnu_value, neo_value) =
        declining_probe_scene("P640SHORT", "(make-string 30 ?a)", false, false);
    assert_eq!(
        gnu_value, "(1148 10 nil)",
        "GNU's DSHORT probe row adds no stop"
    );
    assert_eq!(neo_value, gnu_value, "DSHORT probe row must match GNU");
}

/// DTRUNC400: `truncate-lines` set on 400-column lines.  The probe row covers
/// the TRUNCATED tail of line 38, so GNU enters the newline inside that
/// skipped tail (15238); this is the one row that exercises the truncated
/// row path of the probe measurement.
#[test]
fn declining_fontification_probes_truncated_tail_like_gnu() {
    let (gnu_value, neo_value) =
        declining_probe_scene("P640TRUNC", "(make-string 400 ?a)", true, false);
    assert_eq!(
        gnu_value, "(14838 38 (15238))",
        "GNU's DTRUNC400 probe row enters the truncated tail's newline"
    );
    assert_eq!(neo_value, gnu_value, "DTRUNC400 probe row must match GNU");
}

/// DFORCE: a forced-start walk gets NO probe (segment 1: no 5738), while the
/// NOFORCE t walk does (segment 2), and an ordinary walk does (segment 0).
#[test]
fn declining_fontification_probe_skips_forced_start_like_gnu() {
    let (gnu_value, neo_value) =
        declining_probe_scene("P640FORCE", "(make-string 150 ?a)", false, true);
    assert_eq!(
        gnu_value, "(5588 (38 (5738)) (38 nil) (38 (5738)))",
        "GNU's DFORCE forced pass has no probe row"
    );
    assert_eq!(neo_value, gnu_value, "DFORCE probe gating must match GNU");
}
