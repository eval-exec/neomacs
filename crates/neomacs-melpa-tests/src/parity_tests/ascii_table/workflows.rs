//! The `M-x ascii-table` workflow driven against real windows.
//!
//! Every other file in this corpus reaches the layout loop through a fake:
//! `ascii-table--width-limit` replaced by a constant, or `walk-windows`,
//! `window-buffer` and `window-width` replaced by a table of made-up pairs.
//! That pins the arithmetic but never witnesses the package's whole point,
//! which is that the table it draws is the widest one that fits the window
//! the user is actually looking at. A batch frame has real windows, so the
//! documented workflow -- `M-x ascii-table`, narrow the window, press `g` --
//! runs here with no substitute anywhere in the path.
//!
//! Both workflows derive the chosen layout as codepoints per row rather than
//! comparing rendered text alone, so what is being asserted is the decision
//! the package made and not just the bytes it happened to emit.

use expect_test::expect;

use super::ParityBatchCase;

/// `M-x ascii-table` in a real frame, then narrow the real window and revert.
///
/// Each snapshot is (TAG WINDOWS WIDTH-LIMIT PAIRS-PER-ROW THIRD-LINE
/// WIDEST-LINE POINT READ-ONLY). The third line holds the first row of table
/// data, and the pairs-per-row figure is recovered from how many rows the
/// package needed for 128 codepoints.
///
/// The middle snapshot is the one worth having. Narrowing the window changes
/// `ascii-table--width-limit` from 80 to 49 immediately, and the buffer text
/// does not move -- the table only re-fits itself when something calls the
/// revert function. A user who drags a window divider and wonders why the
/// columns still overhang is seeing this, and it is the behaviour a fake
/// width limit cannot show, because a fake is only ever read at render time.
///
/// The trailing flags state the claims as predicates rather than leaving them
/// to be read out of the text: narrowing alone did not relayout, `g` did, and
/// what `g` produced fits inside the window that was measured.
fn narrowing_a_real_window_relayouts_the_table_only_once_g_reverts_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "narrowing_a_real_window_relayouts_the_table_only_once_g_reverts_it",
        r##"(let* ((snapshot
                   (lambda (tag)
                     (with-current-buffer "*ASCII*"
                       (let* ((text (buffer-string))
                              (lines (split-string text "\n")))
                         (list
                          tag
                          (mapcar
                           (lambda (window)
                             (cons
                              (buffer-name (window-buffer window))
                              (window-width window)))
                           (window-list))
                          (ascii-table--width-limit)
                          (ceiling 128 (- (length lines) 3))
                          (nth 2 lines)
                          (apply #'max (mapcar #'length lines))
                          (point)
                          buffer-read-only)))))
                  (ascii-table-base 16)
                  (ascii-table-control nil)
                  (ascii-table-escape nil)
                  report)
             (when (get-buffer "*ASCII*")
               (kill-buffer "*ASCII*"))
             (unwind-protect
                 (progn
                   (ascii-table)
                   (push (funcall snapshot :after-m-x) report)
                   (set-window-buffer
                    (split-window-horizontally 50)
                    (get-buffer-create "*scratch*"))
                   (push (funcall snapshot :after-narrowing) report)
                   (call-interactively (key-binding (kbd "g") t))
                   (push (funcall snapshot :after-g) report)
                   (setq report (nreverse report))
                   (list
                    report
                    :narrowing-alone-changed-the-table
                    (not (equal (nth 4 (nth 0 report))
                                (nth 4 (nth 1 report))))
                    :g-changed-the-table
                    (not (equal (nth 4 (nth 1 report))
                                (nth 4 (nth 2 report))))
                    :new-layout-fits-the-window
                    (< (nth 5 (nth 2 report))
                       (nth 2 (nth 2 report)))))
               (when (get-buffer "*ASCII*")
                 (kill-buffer "*ASCII*"))
               (delete-other-windows)))"##,
        expect![[
            r#"OK (((:after-m-x (("*ASCII*" . 80) ("*scratch*" . 80)) 80 8 "00  NUL  10  DLE  20     30  0  40  @  50  P  60  `  70  p  " 60 1 t) (:after-narrowing (("*ASCII*" . 49) ("*scratch*" . 30) ("*scratch*" . 80)) 49 8 "00  NUL  10  DLE  20     30  0  40  @  50  P  60  `  70  p  " 60 1 t) (:after-g (("*ASCII*" . 49) ("*scratch*" . 30) ("*scratch*" . 80)) 49 6 "00  NUL  16  SYN  2C  ,  42  B  58  X  6E  n  " 46 1 t)) :narrowing-alone-changed-the-table nil :g-changed-the-table t :new-layout-fits-the-window t)"#
        ]],
    )
}

fn reverting_with_two_real_windows_fits_the_narrowest_one_not_the_selected_one() -> ParityBatchCase
{
    ParityBatchCase::value(
        "reverting_with_two_real_windows_fits_the_narrowest_one_not_the_selected_one",
        r##"(let* ((widest-layout-fitting
                   (lambda (limit)
                     (cl-loop
                      for pairs in '(8 7 6 5 4 3 2 1)
                      for widths = (ascii-table--column-widths
                                    (ascii-table--table pairs)
                                    (* 2 pairs))
                      when (< (+ (cl-reduce #'+ widths)
                                 (* 2 (length widths)))
                              limit)
                      return pairs)))
                  (snapshot
                   (lambda (tag)
                     (with-current-buffer "*ASCII*"
                       (let* ((text (buffer-string))
                              (lines (split-string text "\n")))
                         (list
                          tag
                          (mapcar
                           (lambda (window)
                             (cons
                              (buffer-name (window-buffer window))
                              (window-width window)))
                           (window-list))
                          (window-width (selected-window))
                          (ascii-table--width-limit)
                          (ceiling 128 (- (length lines) 3))
                          (nth 2 lines))))))
                  (ascii-table-base 16)
                  (ascii-table-control nil)
                  (ascii-table-escape nil)
                  report)
             (when (get-buffer "*ASCII*")
               (kill-buffer "*ASCII*"))
             (unwind-protect
                 (let ((narrow nil)
                       (two nil)
                       (one nil))
                   (ascii-table)
                   (setq narrow (split-window-horizontally 50))
                   (push
                    (list
                     :both-windows-show
                     (buffer-name (window-buffer narrow))
                     (buffer-name (window-buffer (selected-window))))
                    report)
                   (call-interactively (key-binding (kbd "g") t))
                   (setq two (funcall snapshot :two-windows))
                   (push two report)
                   (delete-window narrow)
                   (call-interactively (key-binding (kbd "g") t))
                   (setq one (funcall snapshot :one-window))
                   (push one report)
                   (list
                    (nreverse report)
                    :rendered-layout (nth 4 two)
                    :narrowest-window-admits
                    (funcall widest-layout-fitting (nth 3 two))
                    :selected-window-alone-would-admit
                    (funcall widest-layout-fitting (nth 2 two))
                    :widened-again
                    (list (nth 4 two) (nth 4 one))))
               (when (get-buffer "*ASCII*")
                 (kill-buffer "*ASCII*"))
               (delete-other-windows)))"##,
        expect![[
            r#"OK (((:both-windows-show "*ASCII*" "*ASCII*") (:two-windows (("*ASCII*" . 49) ("*ASCII*" . 30) ("*scratch*" . 80)) 49 30 3 "00  NUL  2B  +  56  V  ") (:one-window (("*ASCII*" . 80) ("*scratch*" . 80)) 80 80 8 "00  NUL  10  DLE  20     30  0  40  @  50  P  60  `  70  p  ")) :rendered-layout 3 :narrowest-window-admits 3 :selected-window-alone-would-admit 6 :widened-again (3 8))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        narrowing_a_real_window_relayouts_the_table_only_once_g_reverts_it(),
        reverting_with_two_real_windows_fits_the_narrowest_one_not_the_selected_one(),
    ]
}
