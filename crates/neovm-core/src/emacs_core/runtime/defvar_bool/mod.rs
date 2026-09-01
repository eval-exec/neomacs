//! GNU's `DEFVAR_BOOL' declarations, in one table.
//!
//! `DEFVAR_BOOL' (`src/lisp.h:3507-3512') binds a Lisp symbol to a C `bool *'
//! and hands the descriptor to `defvar_bool' (`src/lread.c:5253-5262').  Two
//! things follow from that, and Neomacs needs both:
//!
//! 1. **The slot coerces.**  `store_symval_forwarding''s `Lisp_Fwd_Bool' arm is
//!    `*XBOOLVAR (valcontents) = !NILP (newval);' (`src/data.c:1485-1487') and
//!    `do_symval_forwarding' rebuilds `t' or `nil' on the way out
//!    (`src/data.c:1337-1360'), so what you put in is not what you get out.
//!    `(setq visible-bell 5)' returns 5 and `visible-bell' then reads `t'.
//!    There is no signal: unlike `Lisp_Fwd_Int' this arm cannot fail.
//!
//! 2. **The byte optimizer has to be told.**  `defvar_bool' conses the symbol
//!    onto `Vbyte_boolean_vars' (`src/lread.c:5261'), and `byte-optimize-lapcode'
//!    checks that list before folding a `varset X; varref X' pair into the value
//!    it stored -- "because varset may change the value" for a Boolean variable
//!    (`lisp/emacs-lisp/byte-opt.el:2285-2300'), where it substitutes `t'
//!    instead.  Without the list, a byte-compiled
//!    `(progn (setq inhibit-message 4) inhibit-message)' returns 4 where GNU
//!    returns `t'.  This is the coercion rule reaching the compiler, so it
//!    belongs to the declaration, not to a caller.
//!
//! # Why the list is shorter than the set
//!
//! `byte-boolean-vars' does NOT hold every `DEFVAR_BOOL' variable, despite its
//! docstring.  `syms_of_lread' declares the list and then writes
//! `Vbyte_boolean_vars = Qnil;' (`src/lread.c:5772-5774'), which discards every
//! cons `defvar_bool' has made so far.  `main' calls thirteen `syms_of_*'
//! functions before `syms_of_lread' -- xfaces, keymap, keyboard, data, fns,
//! fileio, alloc, charset, coding, textconv, comp, callproc, chartab
//! (`src/emacs.c:1976-2307') -- and `syms_of_lread' itself declares two
//! `DEFVAR_BOOL's above the list.  Those 31 declarations are erased again
//! before any Lisp can look at them.
//!
//! Measured under GNU Emacs 31.0.90 on GNU/Linux, `-Q --batch': 148
//! `DEFVAR_BOOL' variables are bound, and `(length byte-boolean-vars)' is 117.
//! Neomacs has no `main' whose call order could reproduce that accident, so
//! [`ByteBooleanVars`] states it instead -- a required field of every row, not
//! a fact a registration site can forget.
//!
//! # Why a table
//!
//! GNU's 148 declarations are spread over 24 `syms_of_*' functions, and the one
//! thing that distinguishes them for [`ByteBooleanVars`] is *which* function --
//! a fact that is invisible at any single site.  Collecting them keeps the GNU
//! source citation, the initial value and the byte-optimizer visibility of each
//! variable on one line, and makes the two counts checkable in a test rather
//! than re-derived per site.

use super::symbol::Obarray;

/// Whether `defvar_bool''s cons onto `byte-boolean-vars' survives to Lisp.
///
/// GNU does not decide this per variable -- it falls out of the order `main'
/// runs the `syms_of_*' functions in.  Naming it is the only way Neomacs can
/// reproduce the resulting list without reproducing GNU's whole startup
/// sequence, and making it a field of [`GnuBoolVariable`] rather than a
/// defaulted argument is what stops a new row from silently guessing.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ByteBooleanVars {
    /// Declared after `Vbyte_boolean_vars = Qnil' (`src/lread.c:5774'), so the
    /// cons survives and the byte optimizer refuses to fold `varset'/`varref'.
    /// 117 variables.
    Listed,
    /// Declared before it, so the cons is thrown away again and the byte
    /// optimizer folds the pair exactly as it would for a plain variable --
    /// even though the slot still coerces at run time.  31 variables.
    ErasedByLreadInit,
}

/// One GNU `DEFVAR_BOOL' declaration.
#[derive(Copy, Clone, Debug)]
pub struct GnuBoolVariable {
    /// The Lisp symbol name `DEFVAR_BOOL''s first argument spells.
    pub name: &'static str,
    /// The value GNU's C declaration leaves in the slot.  A `bool', not a
    /// [`Value`](super::value::Value), for the same reason GNU's slot is a
    /// `bool *': a `DEFVAR_BOOL' variable seeded with something that is not a
    /// Boolean is not a state either program can be in.
    pub initial: bool,
    /// Whether the byte optimizer is told about the coercion.
    pub byte_boolean_vars: ByteBooleanVars,
}

impl GnuBoolVariable {
    const fn listed(name: &'static str, initial: bool) -> Self {
        Self {
            name,
            initial,
            byte_boolean_vars: ByteBooleanVars::Listed,
        }
    }

    const fn erased(name: &'static str, initial: bool) -> Self {
        Self {
            name,
            initial,
            byte_boolean_vars: ByteBooleanVars::ErasedByLreadInit,
        }
    }
}

use GnuBoolVariable as V;

/// Every `DEFVAR_BOOL' GNU Emacs 31.0.90 declares on GNU/Linux, in declaration
/// order.  The trailing comment on each row is the GNU `file:line' the
/// declaration is at.
///
/// Registering in this order reproduces GNU's `byte-boolean-vars' exactly,
/// because `defvar_bool' prepends: the last row registered ends up first.
///
/// Not listed here are the 36 `DEFVAR_BOOL's that belong to builds this one is
/// not -- w32, Haiku, Android, `sfntfont', native compilation, xwidgets --
/// which GNU leaves unbound here too.
pub static GNU_BOOL_VARIABLES: &[GnuBoolVariable] = &[
    // ---- Erased again by `syms_of_lread' itself (31) ----
    // `main' runs these `syms_of_*' before `syms_of_lread'
    // (`src/emacs.c:1976-2306'), so `Vbyte_boolean_vars = Qnil'
    // (`src/lread.c:5774') throws their conses away.
    V::erased("face-filters-always-match", false), // xfaces.c:7612
    V::erased("kill-emacs-on-sigint", true),       // keyboard.c:13411
    V::erased("auto-save-no-message", false),      // keyboard.c:13846
    V::erased("echo-keystrokes-help", true),       // keyboard.c:13863
    V::erased("cannot-suspend", false),            // keyboard.c:13973
    V::erased("menu-prompting", true),             // keyboard.c:13978
    V::erased("attempt-stack-overflow-recovery", true), // keyboard.c:14369
    V::erased("attempt-orderly-shutdown-on-fatal-signal", true), // keyboard.c:14378
    V::erased("translate-upper-case-key-bindings", true), // keyboard.c:14398
    V::erased("input-pending-p-filter-events", true), // keyboard.c:14408
    V::erased("mwheel-coalesce-scroll-events", true), // keyboard.c:14416
    V::erased("inhibit--record-char", false),      // keyboard.c:14431
    V::erased("record-all-keys", false),           // keyboard.c:14439
    V::erased("disable-inhibit-text-conversion", false), // keyboard.c:14452
    V::erased("multiple-terminals-merge-keyboards", false), // keyboard.c:14468
    V::erased("symbols-with-pos-enabled", false),  // data.c:4386
    V::erased("use-dialog-box", true),             // fns.c:6836
    V::erased("use-file-dialog", true),            // fns.c:6845
    V::erased("use-short-answers", false),         // fns.c:6853
    V::erased("write-region-inhibit-fsync", true), // fileio.c:6951
    V::erased("delete-by-moving-to-trash", false), // fileio.c:6971
    V::erased("garbage-collection-messages", false), // alloc.c:7459
    V::erased("inhibit-load-charset-map", false),  // charset.c:2430
    V::erased("inhibit-eol-conversion", false),    // coding.c:12022
    V::erased("inherit-process-coding-system", false), // coding.c:12029
    V::erased("coding-system-require-warning", false), // coding.c:12172
    V::erased("inhibit-iso-escape-detection", false), // coding.c:12181
    V::erased("inhibit-null-byte-detection", false), // coding.c:12209
    V::erased("disable-ascii-optimization", false), // coding.c:12222
    V::erased("load-in-progress", false),          // lread.c:5670
    V::erased("load-force-doc-strings", false),    // lread.c:5756
    // ---- On `byte-boolean-vars' (117) ----
    // In GNU declaration order, so consing them in this order
    // reproduces GNU's list exactly (`defvar_bool' prepends).
    V::listed("load-dangerous-libraries", false), // lread.c:5776
    V::listed("force-load-messages", false),      // lread.c:5783
    V::listed("load-prefer-newer", false),        // lread.c:5824
    V::listed("load-no-native", false),           // lread.c:5836
    V::listed("print-integers-as-characters", false), // print.c:2910
    V::listed("print-escape-newlines", false),    // print.c:2928
    V::listed("print-escape-control-characters", false), // print.c:2933
    V::listed("print-escape-nonascii", false),    // print.c:2938
    V::listed("print-escape-multibyte", false),   // print.c:2946
    V::listed("print-quoted", true),              // print.c:2952
    V::listed("print-symbols-bare", false),       // print.c:3015
    V::listed("debug-on-quit", false),            // eval.c:4491
    V::listed("debug-on-next-call", false),       // eval.c:4496
    V::listed("backtrace-on-redisplay-error", false), // eval.c:4499
    V::listed("debugger-may-continue", true),     // eval.c:4504
    V::listed("debugger-stack-frame-as-list", false), // eval.c:4510
    V::listed("backtrace-on-error-noninteractive", true), // eval.c:4539
    V::listed("kill-buffer-delete-auto-save-files", false), // buffer.c:5969
    V::listed("delete-auto-save-files", true),    // buffer.c:5976
    V::listed("inhibit-mouse-event-check", false), // callint.c:917
    V::listed("case-symbols-as-words", false),    // casefiddle.c:738
    V::listed("inverse-video", false),            // dispnew.c:7493
    V::listed("visible-bell", false),             // dispnew.c:7497
    V::listed("no-redraw-on-reenter", false),     // dispnew.c:7502
    V::listed("cursor-in-echo-area", false),      // dispnew.c:7539
    V::listed("mouse-prefer-closest-glyph", false), // dispnew.c:7542
    V::listed("documentation-dynamic-reload", true), // doc.c:720
    V::listed("internal--text-quoting-flag", true), // doc.c:735
    // `noninteractive1 = noninteractive' (`src/emacs.c:1953') -- argv, not a
    // constant.  neomacs seeds a batch-shaped default here for contexts that
    // never see a command line, and `load::init_command_line_vars' overwrites
    // it from the parsed argv in the real binary.
    V::listed("noninteractive", true),              // emacs.c:3535
    V::listed("inhibit-x-resources", false),        // emacs.c:3597
    V::listed("create-lockfiles", true),            // filelock.c:818
    V::listed("indent-tabs-mode", true),            // indent.c:2575
    V::listed("inhibit-modification-hooks", false), // insdel.c:2575
    V::listed("read-buffer-completion-ignore-case", false), // minibuf.c:2548
    V::listed("history-delete-duplicates", false),  // minibuf.c:2572
    V::listed("completion-ignore-case", false),     // minibuf.c:2585
    V::listed("enable-recursive-minibuffers", false), // minibuf.c:2593
    V::listed("minibuffer-auto-raise", false),      // minibuf.c:2654
    V::listed("minibuffer-allow-text-properties", false), // minibuf.c:2673
    V::listed("inhibit-interaction", false),        // minibuf.c:2697
    V::listed("read-minibuffer-restore-windows", true), // minibuf.c:2706
    V::listed("delete-exited-processes", true),     // process.c:8916
    V::listed("process-prioritize-lower-fds", false), // process.c:8943
    V::listed("fast-read-process-output", true),    // process.c:8980
    V::listed("current-time-list", true),           // timefns.c:2112
    V::listed("scroll-bar-adjust-thumb-portion", true), // frame.c:7465
    V::listed("frame-resize-pixelwise", false),     // frame.c:7623
    V::listed("tooltip-reuse-hidden-frame", false), // frame.c:7710
    V::listed("use-system-tooltips", true),         // frame.c:7725
    V::listed("after-delete-frame-select-mru-frame", true), // frame.c:7796
    V::listed("parse-sexp-ignore-comments", false), // syntax.c:3764
    V::listed("parse-sexp-lookup-properties", false), // syntax.c:3767
    V::listed("words-include-escapes", false),      // syntax.c:3780
    V::listed("multibyte-syntax-as-symbol", false), // syntax.c:3783
    V::listed("open-paren-in-column-0-is-defun-start", true), // syntax.c:3787
    V::listed("comment-end-can-be-escaped", false), // syntax.c:3811
    V::listed("system-uses-terminfo", true),        // term.c:5224
    V::listed("visible-cursor", true),              // term.c:5246
    V::listed("tty-menu-calls-mouse-position-function", false), // term.c:5253
    V::listed("tty-cursor-movement-use-TAB-BS", false), // term.c:5260
    V::listed("undo-inhibit-record-point", false),  // undo.c:487
    V::listed("mode-line-in-non-selected-windows", true), // window.c:9225
    V::listed("auto-window-vscroll", true),         // window.c:9243
    V::listed("window-resize-pixelwise", false),    // window.c:9519
    V::listed("fast-but-imprecise-scrolling", false), // window.c:9530
    V::listed("window-auto-redraw-on-parameter-change", true), // window.c:9549
    V::listed("scroll-minibuffer-conservatively", true), // xdisp.c:38208
    V::listed("inhibit-message", false),            // xdisp.c:38215
    V::listed("x-stretch-cursor", false),           // xdisp.c:38423
    V::listed("nobreak-char-ascii-display", false), // xdisp.c:38459
    V::listed("word-wrap-by-category", false),      // xdisp.c:38579
    V::listed("highlight-nonselected-windows", false), // xdisp.c:38609
    V::listed("multiple-frames", false),            // xdisp.c:38615
    V::listed("auto-raise-tab-bar-buttons", true),  // xdisp.c:38704
    V::listed("auto-raise-tool-bar-buttons", true), // xdisp.c:38717
    V::listed("make-window-start-visible", false),  // xdisp.c:38731
    V::listed("tab-bar-truncate", false),           // xdisp.c:38737
    V::listed("unibyte-display-via-language-environment", false), // xdisp.c:38815
    V::listed("message-truncate-lines", false),     // xdisp.c:38898
    V::listed("inhibit-menubar-update", false),     // xdisp.c:38915
    V::listed("display-line-numbers-widen", false), // xdisp.c:38993
    V::listed("display-fill-column-indicator", false), // xdisp.c:39008
    V::listed("inhibit-eval-during-redisplay", false), // xdisp.c:39050
    V::listed("inhibit-free-realized-faces", false), // xdisp.c:39054
    V::listed("inhibit-bidi-mirroring", false),     // xdisp.c:39058
    V::listed("bidi-inhibit-bpa", false),           // xdisp.c:39064
    V::listed("display-hourglass", true),           // xdisp.c:39102
    V::listed("redisplay--inhibit-bidi", true),     // xdisp.c:39235
    V::listed("display-raw-bytes-as-hex", false),   // xdisp.c:39241
    V::listed("mouse-fine-grained-tracking", false), // xdisp.c:39247
    V::listed("tab-bar--dragging-in-progress", false), // xdisp.c:39256
    V::listed("redisplay-skip-initial-frame", true), // xdisp.c:39260
    V::listed("redisplay-skip-fontification-on-input", false), // xdisp.c:39268
    V::listed("redisplay-adhoc-scroll-in-resize-mini-windows", true), // xdisp.c:39281
    V::listed("composition-break-at-point", false), // xdisp.c:39289
    V::listed("inhibit-compacting-font-caches", false), // font.c:6019
    V::listed("xft-ignore-color-fonts", true),      // font.c:6039
    V::listed("query-all-font-backends", false),    // font.c:6046
    V::listed("cross-disabled-images", false),      // image.c:13259
    V::listed("x-use-underline-position-properties", true), // xterm.c:32667
    V::listed("x-underline-at-descent-line", false), // xterm.c:32678
    V::listed("x-mouse-click-focus-ignore-position", false), // xterm.c:32689
    V::listed("x-frame-normalize-before-maximize", false), // xterm.c:32812
    V::listed("x-gtk-use-window-move", true),       // xterm.c:32825
    V::listed("x-gtk-use-native-input", false),     // xterm.c:32839
    V::listed("x-input-grab-touch-events", true),   // xterm.c:32860
    V::listed("x-dnd-fix-motif-leave", true),       // xterm.c:32870
    V::listed("x-dnd-disable-motif-drag", false),   // xterm.c:32878
    V::listed("x-dnd-preserve-selection-data", false), // xterm.c:32947
    V::listed("x-dnd-disable-motif-protocol", false), // xterm.c:32954
    V::listed("x-dnd-use-unsupported-drop", true),  // xterm.c:32960
    V::listed("x-fast-protocol-requests", false),   // xterm.c:32967
    V::listed("x-gtk-use-old-file-dialog", false),  // xfns.c:10460
    V::listed("x-gtk-show-hidden-files", false),    // xfns.c:10467
    V::listed("x-gtk-file-dialog-help-text", true), // xfns.c:10473
    V::listed("use-default-font-for-symbols", true), // fontset.c:2206
    V::listed("font-use-system-font", false),       // xsettings.c:1395
];

/// Install every row as a GNU `DEFVAR_BOOL' variable.
///
/// Runs FIRST among the bootstrap registrations, for the reason `main' runs
/// every `syms_of_*' before Lisp: `Fmake_variable_buffer_local' copies the
/// symbol's forwarder into the BLV (`src/data.c:2112-2140'), so a variable
/// that gets localized further down -- `indent-tabs-mode',
/// `display-fill-column-indicator', `display-line-numbers-widen',
/// `make-window-start-visible', `case-symbols-as-words',
/// `comment-end-can-be-escaped' -- has to be forwarded before that happens.
/// Running first also makes the table, and not a plain-cell seed elsewhere,
/// what a `DEFVAR_BOOL' variable's default comes from; the values these
/// variables acquire later from the command line or from `startup.el' --
/// `noninteractive', `inhibit-x-resources', `redisplay--inhibit-bidi' -- go
/// through the forwarder like any other assignment.
pub fn register_bootstrap_vars(obarray: &mut Obarray) {
    for var in GNU_BOOL_VARIABLES {
        obarray.define_bool_variable(var.name, var.initial, var.byte_boolean_vars);
    }
}

// The Boolean descriptors a portable dump cannot carry used to be rebuilt from
// this table on load.  They are not any more: `DumpSymbolVal::Localized` records
// which forwarder `make_blv` copied into the BLV (`src/data.c:2112-2140`) and
// `load_obarray` rebuilds it from the image, so a variable Lisp localized keeps
// its coercion without anyone having to remember to list it here.  See
// `DumpLocalizedForwarder` (pdump format v58).

#[cfg(test)]
mod tests {
    use super::*;

    /// Measured under GNU Emacs 31.0.90, `-Q --batch': 148 `DEFVAR_BOOL'
    /// variables are bound and 117 of them are on `byte-boolean-vars'.
    #[test]
    fn table_matches_gnu_counts() {
        assert_eq!(GNU_BOOL_VARIABLES.len(), 148);
        assert_eq!(
            GNU_BOOL_VARIABLES
                .iter()
                .filter(|v| v.byte_boolean_vars == ByteBooleanVars::Listed)
                .count(),
            117
        );
    }

    #[test]
    fn table_has_no_duplicate_rows() {
        let mut names: Vec<&str> = GNU_BOOL_VARIABLES.iter().map(|v| v.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate DEFVAR_BOOL row");
    }
}
