//! Re-homed FULL display xdisp tests (runtime eval terminals). Test bodies
//! are verbatim from the display source; no display-private item is used.

#[test]
fn tty_right_divider_parameter_does_not_change_geometry_or_border_hit() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = eval
        .eval_str(
            r##"(progn
                  (split-window-right)
                  (let* ((left (frame-first-window))
                         (before-right (nth 2 (window-inside-pixel-edges left))))
                    (set-frame-parameter nil 'right-divider-width 8)
                    (redisplay t)
                    (let* ((after-right (nth 2 (window-inside-pixel-edges left)))
                           (y (+ 10 (nth 1 (window-inside-pixel-edges left)))))
                      (list before-right
                            after-right
                            (frame-parameter nil 'right-divider-width)
                            (frame-right-divider-width)
                            (window-right-divider-width left)
                            (posn-area (posn-at-x-y after-right y nil t))))))"##,
        )
        .expect("issue 299 terminal divider probe");

    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "(39 39 8 0 0 vertical-line)"
    );
}
/// `window-text-pixel-size` must honour its Y-LIMIT argument.
///
/// GNU's signature is
/// `(WINDOW &optional FROM TO X-LIMIT Y-LIMIT MODE-LINES IGNORE-LINE-AT-END)`,
/// and Y-LIMIT bounds how far `move_it_to' walks (src/xdisp.c).  We read
/// MODE-LINES at argument 5 but never read Y-LIMIT at argument 4, handing the
/// scanner `None` and walking BEGV..ZV however large the buffer was.
///
/// That is what made `fit-window-to-buffer' scan the whole buffer: its
/// vertical branch passes `(frame-pixel-height frame)' as Y-LIMIT
/// (lisp/window.el).  200 calls over a 320,000-character buffer took 2,154ms
/// against GNU's flat 31ms.
///
/// The answers change, not just the timing, so every row here is pinned to
/// GNU Emacs 31.1.  Y-LIMIT is in PIXELS while the scanner caps LINES, and
/// GNU counts the row that REACHES the limit -- with a char height of 1,
/// Y-LIMIT 5 yields 6 rows, not 5.  The increasing sweep is what pins that
/// conversion; a single row would pass with an off-by-one.
#[test]
fn window_text_pixel_size_honours_its_y_limit_like_gnu() {
    crate::test_utils::init_test_tracing();
    let observed = crate::test_utils::runtime_startup_eval_one(
        r#"(with-current-buffer (get-buffer-create "wt-ylimit")
             (erase-buffer)
             (dotimes (i 300) (insert (format "line %d of some text\n" i)))
             (set-window-buffer (selected-window) (current-buffer))
             (list (frame-char-height)
                   (window-text-pixel-size (selected-window) nil t nil nil t)
                   (window-text-pixel-size (selected-window) nil t nil 1 t)
                   (window-text-pixel-size (selected-window) nil t nil 5 t)
                   (window-text-pixel-size (selected-window) nil t nil 50 t)
                   (window-text-pixel-size (selected-window) nil t nil 100000 t)
                   (window-text-pixel-size (selected-window) nil t 40 10 t)
                   (window-text-pixel-size (selected-window) 1 200 nil 5 t)))"#,
    );
    // With an X-LIMIT as well as a Y-LIMIT, GNU counts the width of the row the
    // limit lands in: `(window-text-pixel-size win nil t 40 10 t)` returns
    // (20 . 11), i.e. the 20-character width of row 10 and 11 rows scanned.
    // Attested on the GNU Emacs 31.1 oracle (`linux-gtk3`, commit
    // a360712c9d272d950d8d8255ef74570f7e90b7d9) with exactly the form below:
    //   OK (1 (21 . 301) (19 . 2) (19 . 6) (20 . 51) (21 . 301) (20 . 11) (19 . 6))
    // The y-limit-only cases are unaffected: the same oracle run reproduces
    // (19 . 2), (19 . 6), (20 . 51) and (21 . 301) unchanged.
    // The expectation here was stale: it encoded Neomacs' pre-R3-1 behaviour
    // (19, the width of rows 0..9) while claiming to be the GNU value.  The
    // union row caught the divergence only because `1e490fdc0b` corrected the
    // implementation; the row and the implementation disagreed in the other
    // direction before that.
    assert_eq!(
        observed,
        "OK (1 (21 . 301) (19 . 2) (19 . 6) (20 . 51) (21 . 301) (20 . 11) (19 . 6))"
    );
}
