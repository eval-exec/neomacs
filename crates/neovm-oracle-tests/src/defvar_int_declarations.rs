//! Oracle guards for the GNU `DEFVAR_INT' declarations entry 132 left open,
//! and for the platform names `cus-start.el' mentions but this build does not
//! declare.
//!
//! Three separate things are pinned here, and they fail in different ways:
//!
//! 1. **Existence.**  `cus-start.el' lists every variable GNU's C layer can
//!    define across all its platforms and only signals when one is missing AND
//!    its `native-p' test says this build should have had it
//!    (`lisp/cus-start.el:893-951').  Seeding the others anyway makes `boundp'
//!    answer `t' where GNU answers `nil' -- a divergence in the direction
//!    nothing complains about.
//! 2. **The forward type.**  `DEFVAR_INT' binds the symbol to an `intmax_t *'
//!    (`src/lisp.h:3513-3518'), so `store_symval_forwarding' either stores an
//!    integer or signals: `(wrong-type-argument integerp VAL)' for a
//!    non-integer, `(overflow-error VAL)' for an integer past the slot
//!    (`src/data.c:1475-1483').  A variable declared with the wrong kind gets
//!    that wrong quietly.
//! 3. **`baud-rate' has no initializer and no `init_*' that supplies one**
//!    (`src/dispnew.c:7488').  The only writers are `init_baud_rate' from
//!    `init_tty' (`src/term.c:4755', `4923') and a window system's own
//!    `baud_rate = 19200' (`src/xterm.c:32279', `src/pgtkterm.c:7034').
//!    `--batch' reaches neither, so GNU reports the C global's 0 -- and
//!    `init_baud_rate' could not have produced a 0 anyway, its floor is 1200
//!    (`src/sysdep.c:435-436').

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The names `cus-start.el` mentions whose C file this build does not compile.
/// GNU leaves every one of them unbound on GNU/Linux; the three controls at the
/// end are platform names GNU DOES bind here, so the pin fails if the answer
/// swings the other way and everything gets deleted.
#[test]
fn oracle_platform_variables_gnu_does_not_declare_here_are_unbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (bound)
  (dolist (s '(dos-hyper-key dos-super-key dos-keypad-mode dos-display-scancodes
               dos-unsupported-char-glyph imagemagick-render-type
               xwidget-internal
               w32-follow-system-dark-mode haiku-debug-on-fatal-error
               haiku-use-system-tooltips ns-control-modifier ns-command-modifier
               ns-alternate-modifier ns-function-modifier ns-antialias-text
               ns-auto-hide-menu-bar ns-confirm-quit ns-use-native-fullscreen
               ns-use-srgb-colorspace ns-click-through))
    (when (boundp s) (push s bound)))
  (list (nreverse bound)
        (and (boundp 'window-combination-limit) (boundp 'void-text-area-pointer)
             (boundp 'vertical-centering-font-regexp))))"#;
    let expect = expect_test::expect![r#""OK (nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `baud-rate` under `--batch`: the C global's zero, never `init_baud_rate`'s
/// answer, because `--batch` creates no terminal to initialize.
#[test]
fn oracle_baud_rate_is_zero_in_batch_and_refuses_a_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list baud-rate
      (default-value 'baud-rate)
      (local-variable-if-set-p 'baud-rate)
      (condition-case e (setq baud-rate "x") (error (car e)))
      baud-rate
      (condition-case e (makunbound 'baud-rate) (error (car e))))"#;
    let expect = expect_test::expect![r#""OK (0 0 nil wrong-type-argument 0 error)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `display-line-numbers-offset` is declared BOTH ways and in this order
/// (`src/xdisp.c:38999-39005`): `DEFVAR_INT`, then
/// `Fmake_variable_buffer_local`.  `make_blv` copies the forwarder into the BLV
/// (`src/data.c:2112-2140`), so the integer rule applies per buffer as well as
/// to the default -- while `setq-local` with an integer still works and leaves
/// the default alone.
#[test]
fn oracle_display_line_numbers_offset_is_a_buffer_local_defvar_int() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list display-line-numbers-offset
      (local-variable-if-set-p 'display-line-numbers-offset)
      (condition-case e (progn (set-default 'display-line-numbers-offset "x") 'no-error)
        (error (car e)))
      (default-value 'display-line-numbers-offset)
      (condition-case e (let ((display-line-numbers-offset "x"))
                          display-line-numbers-offset)
        (error (car e)))
      (with-temp-buffer
        (list (condition-case e (setq-local display-line-numbers-offset "x") (error (car e)))
              display-line-numbers-offset
              (local-variable-p 'display-line-numbers-offset)))
      (with-temp-buffer
        (setq-local display-line-numbers-offset 3)
        (list display-line-numbers-offset (default-value 'display-line-numbers-offset)))
      (progn (setq-default display-line-numbers-offset 7)
             (list (default-value 'display-line-numbers-offset)
                   (with-temp-buffer display-line-numbers-offset)))
      (condition-case e (makunbound 'display-line-numbers-offset) (error (car e)))
      (progn (setq-default display-line-numbers-offset 0)
             (default-value 'display-line-numbers-offset)))"#;
    let expect = expect_test::expect![
        r#""OK (0 t wrong-type-argument 0 wrong-type-argument (wrong-type-argument 0 t) (3 0) (7 7) error 0)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `syntax-propertize--done` is GNU's other `DEFVAR_INT` +
/// `Fmake_variable_buffer_local` pair (`src/syntax.c:3773-3778`), and it is the
/// one that shows the failure was in the dump rather than in the declaration:
/// Neomacs already localized it with the forwarder copied across, and the
/// portable image -- which could not carry a descriptor -- handed it back
/// without one.  A pin here is worth having because the oracle always runs a
/// dumped binary.
#[test]
fn oracle_syntax_propertize_done_keeps_its_integer_slot_through_the_dump() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (default-value 'syntax-propertize--done)
      (local-variable-if-set-p 'syntax-propertize--done)
      (condition-case e (set-default 'syntax-propertize--done "x") (error (car e)))
      (with-temp-buffer
        (list (condition-case e (setq-local syntax-propertize--done "x") (error (car e)))
              syntax-propertize--done))
      (with-temp-buffer
        (setq-local syntax-propertize--done 12)
        (list syntax-propertize--done (default-value 'syntax-propertize--done))))"#;
    let expect =
        expect_test::expect![r#""OK (-1 t wrong-type-argument (wrong-type-argument -1) (12 -1))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The eight `DEFVAR_INT` variables entry 132 found Neomacs did not define at
/// all: each is bound, special, and refuses a string with GNU's own signal.
#[test]
fn oracle_every_remaining_gnu_defvar_int_is_bound_special_and_typed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((names '(command-line-max-length large-hscroll-threshold
               long-line-optimizations-bol-search-limit
               long-line-optimizations-region-size max-redisplay-ticks
               strings-consed x-color-cache-bucket-size
               x-mouse-click-focus-ignore-time))
      (unbound '()) (nonspecial '()) (accepted '()))
  (dolist (s names)
    (cond ((not (boundp s)) (push s unbound))
          ((not (special-variable-p s)) (push s nonspecial))
          (t (let ((old (default-value s)))
               (unless (eq 'wrong-type-argument
                           (condition-case e (progn (set-default s "x") 'no-error)
                             (error (car e))))
                 (push s accepted))
               (set-default s old)))))
  (list (length names) (nreverse unbound) (nreverse nonspecial) (nreverse accepted)))"#;
    let expect = expect_test::expect![r#""OK (8 nil nil nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// Their GNU initial values.  `command-line-max-length` is
/// `sysconf (_SC_ARG_MAX) / 4` (`src/callproc.c:2246-2252`) and
/// `strings-consed` is a running allocation counter (`src/alloc.c:7448`), so
/// both are asserted by shape: pinning either number would pin this machine
/// into the expectation, which is the mistake entries 127, 129 and 133 record.
#[test]
fn oracle_remaining_defvar_int_initial_values_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list large-hscroll-threshold
      long-line-optimizations-bol-search-limit
      long-line-optimizations-region-size
      max-redisplay-ticks
      x-color-cache-bucket-size
      x-mouse-click-focus-ignore-time
      (and (integerp command-line-max-length) (> command-line-max-length 0))
      (and (integerp strings-consed) (>= strings-consed 0)))"#;
    let expect = expect_test::expect![r#""OK (10000 128 500000 0 128 200 t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `command-line-max-length` asserted by its DERIVATION rather than by its
/// shape, because the shape is what let entry 138 leave the number unexplained.
///
/// The two editors report different numbers on this machine -- GNU 626432,
/// Neomacs 1572864 -- and the declaration is identical: both compute
/// `sysconf (_SC_ARG_MAX) / 4`, GNU's "crude way to go bytes->characters"
/// (`src/callproc.c:2246-2252`).  glibc derives `_SC_ARG_MAX` from
/// RLIMIT_STACK as `MAX (131072, MIN (stack / 4, 6 MiB))`, and both editors
/// raise their own RLIMIT_STACK before that `sysconf` runs -- GNU in `main`,
/// to `emacs_re_max_failures * ratio + extra` rounded to a page
/// (`src/emacs.c:1563-1623`, ahead of `syms_of_callproc` at `src/emacs.c:2172`),
/// which is 9788 KiB here; Neomacs to a flat 128 MiB
/// (`crates/neomacs/src/main.rs:increase_stack_limit`), which lands on glibc's
/// 6 MiB cap.  So the variable is not a constant in either editor: it is a
/// report of that editor's stack policy.
///
/// Asking a child what stack limit it inherited and re-deriving the number
/// makes the pin editor-independent and still exact.  It is the assertion that
/// fails if anyone ever "fixes" the difference by writing GNU's 626432 into
/// Rust -- which is this project's most repeated bug and would make the
/// variable disagree with the machine it describes.
#[test]
fn oracle_command_line_max_length_is_derived_from_this_editors_stack_rlimit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((reported (with-temp-buffer
                   (call-process "sh" nil t nil "-c" "ulimit -s")
                   (car (split-string (buffer-string)))))
       (stack (if (equal reported "unlimited")
                  most-positive-fixnum
                (* 1024 (string-to-number reported))))
       (predicted (/ (max 131072 (min (/ stack 4) (* 6 1024 1024))) 4)))
  (list (integerp command-line-max-length)
        (> command-line-max-length 0)
        (= command-line-max-length predicted)))"#;
    let expect = expect_test::expect![r#""OK (t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The whole family in one sweep: every name `grep DEFVAR_INT src/*.c` finds,
/// asked whether it is bound and whether its slot is really an `intmax_t`.
///
/// GNU binds 52 of the 74 here and every one of those refuses a string; the 22
/// it leaves unbound belong to w32, Android, MS-DOS, pgtk and ImageMagick
/// builds, plus `debug-end-pos` which is inside `GLYPH_DEBUG`.  Defining one of
/// those would be invented existence, which is why the unbound list is pinned
/// by name rather than by count.
#[test]
fn oracle_every_gnu_defvar_int_name_is_bound_exactly_where_gnu_binds_it() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((names '(android-display-planes android-keyboard-bell-duration
               android-quit-keycode auto-save-interval baud-rate
               command-line-max-length cons-cells-consed debug-end-pos
               display-line-numbers-major-tick display-line-numbers-minor-tick
               display-line-numbers-offset dos-codepage dos-country-code
               dos-decimal-point dos-hyper-key dos-keyboard-layout
               dos-keypad-mode dos-super-key dos-timezone-offset
               double-click-fuzz executing-kbd-macro-index
               extra-keyboard-modifiers face-near-same-color-threshold
               floats-consed gc-cons-threshold gcs-done gnutls-log-level
               hscroll-margin imagemagick-render-type integer-width
               internal-when-entered-debugger intervals-consed
               large-hscroll-threshold line-number-display-limit-width
               lisp-eval-depth-reserve long-line-optimizations-bol-search-limit
               long-line-optimizations-region-size max-lisp-eval-depth
               max-redisplay-ticks next-screen-context-lines num-input-keys
               num-nonmacro-input-events overline-margin pgtk-selection-timeout
               process-error-pause-time profiler-log-size
               profiler-max-stack-depth pure-bytes-used read-process-output-max
               scroll-conservatively scroll-margin scroll-step
               string-chars-consed strings-consed symbols-consed
               syntax-propertize--done tab-bar-button-relief
               tool-bar-button-relief tool-bar-max-label-size
               underline-minimum-offset undo-limit undo-strong-limit
               vector-cells-consed w32-ansi-code-page w32-mouse-button-tolerance
               w32-mouse-move-interval w32-multibyte-code-page
               w32-num-mouse-buttons w32-pipe-buffer-size w32-pipe-read-delay
               w32-quit-key x-color-cache-bucket-size
               x-mouse-click-focus-ignore-time x-selection-timeout))
      (unbound '()) (untyped '()))
  (dolist (s names)
    (if (not (boundp s))
        (push s unbound)
      (let ((old (default-value s)))
        (unless (eq 'wrong-type-argument
                    (condition-case e (progn (set-default s "x") 'no-error)
                      (error (car e))))
          (push s untyped))
        (condition-case nil (set-default s old) (error nil)))))
  (list (length names) (- (length names) (length unbound))
        (sort (nreverse unbound) #'string<)
        (sort (nreverse untyped) #'string<)))"#;
    let expect = expect_test::expect![
        r#""OK (74 52 (android-display-planes android-keyboard-bell-duration android-quit-keycode debug-end-pos dos-codepage dos-country-code dos-decimal-point dos-hyper-key dos-keyboard-layout dos-keypad-mode dos-super-key dos-timezone-offset imagemagick-render-type pgtk-selection-timeout w32-ansi-code-page w32-mouse-button-tolerance w32-mouse-move-interval w32-multibyte-code-page w32-num-mouse-buttons w32-pipe-buffer-size w32-pipe-read-delay w32-quit-key) nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The two signals `Lisp_Fwd_Int` distinguishes, on variables that had no
/// declaration at all before: `overflow-error` for an integer past `intmax_t`
/// and `wrong-type-argument` for a float, with the slot unchanged either way.
#[test]
fn oracle_remaining_defvar_int_signals_overflow_and_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (condition-case e (setq max-redisplay-ticks (expt 2 200)) (error (car e)))
      max-redisplay-ticks
      (condition-case e (setq large-hscroll-threshold 1.5) (error (car e)))
      large-hscroll-threshold
      ;; An in-range integer still goes through.  Deliberately not
      ;; `most-positive-fixnum': the oracle normalizer rewrites any fixnum past
      ;; 10^12 to 0 ("large fixnums in error data are implementation
      ;; artefacts", `common.rs`), so pinning one would pin the harness instead
      ;; of either editor.
      (progn (setq large-hscroll-threshold 999999)
             large-hscroll-threshold))"#;
    let expect =
        expect_test::expect![r#""OK (overflow-error 0 wrong-type-argument 10000 999999)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
