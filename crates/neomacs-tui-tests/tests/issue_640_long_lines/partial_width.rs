use super::*;
use std::time::Duration;

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
             (message (concat \"P640OF\" \"BEGIN%S\" \"P640OF\" \"END\") \
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
