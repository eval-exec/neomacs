//! Every GNU C variable whose `DEFVAR_*` shares a preprocessor block with an
//! `Fprovide`, and what this build does about it.
//!
//! # The rule, and why it is not `cus-start.el`'s
//!
//! `lisp/cus-start.el` looks like the specification for this question and is
//! not.  Its `native-p` `cond` (`lisp/cus-start.el:893-952`) fires **only when
//! a name is unbound**, and it answers "is that absence expected here?":
//!
//! ```elisp
//! ((string-match-p "\\`x-.*gtk" sym-name)
//!  (featurep 'gtk))
//! ```
//!
//! Read as a two-way rule that says an `x-.*gtk` name must be absent without
//! GTK, and `src/xfns.c:10459` contradicts it in GNU's own words, on the line
//! immediately above the first of four such names:
//!
//! ```text
//! /* This is not ifdef:ed, so other builds than GTK can customize it.  */
//!   DEFVAR_BOOL ("x-gtk-use-old-file-dialog", ...
//! ```
//!
//! So GNU deliberately binds `x-gtk-use-old-file-dialog`,
//! `x-gtk-show-hidden-files`, `x-gtk-file-dialog-help-text` and
//! `x-gtk-resize-child-frames` in an X build with no GTK at all.  `cus-start`'s
//! rule is *sufficient* there, never *necessary*: it only has to be true
//! whenever the name IS missing.  `x-gtk-use-native-input` gets its own,
//! stronger rule at `:911-913` -- `(and (featurep 'x) (featurep 'gtk))` -- for
//! the same reason, because PGTK provides `gtk` without running
//! `syms_of_xterm`.
//!
//! The rule that IS two-way lives in the C.  When a `DEFVAR_*` and an
//! `Fprovide` sit in the same conditional block, one `configure` switch decides
//! both, so **`(boundp 'V)` and `(featurep 'F)` are the same question in every
//! build GNU can produce**.  `src/xfns.c:10539-10549` is the clearest instance
//! -- the `Fprovide` and the `DEFVAR` are two statements apart inside one
//! `#ifdef USE_GTK`:
//!
//! ```text
//!   Fprovide (intern_c_string ("gtk"), Qnil);
//!
//!   DEFVAR_LISP ("gtk-version-string", Vgtk_version_string,
//!                doc: /* Version info for GTK+.  */);
//! ```
//!
//! -- and `src/xfns.c:10552-10558` repeats it for `cairo` and
//! `cairo-version-string`.  The block need not be an inner `#ifdef`: a whole
//! backend file is a block too, because `src/emacs.c:2364-2489` decides which
//! `syms_of_*` runs.  `syms_of_xfns` runs only under `HAVE_X_WINDOWS`
//! (`emacs.c:2375`) and `Fprovide (Qx, Qnil)` is inside it (`xfns.c:10498`),
//! so every `xterm.c` / `xfns.c` / `xselect.c` name is coupled to
//! `(featurep 'x)` the same way.
//!
//! The rule is falsifiable, and was falsified against GNU before it was used
//! here.  Applied to GNU 31.0.90 (gtk3) with GNU's own measured answers it
//! names **150** variables that build cannot have, and GNU binds **0** of
//! them.  Applied to this build's answers it names **234**, of which this
//! build bound **76**.
//!
//! # The two names it cost, and the one it saved
//!
//! `gtk-version-string` was bound here to the string `"3.24.51"` and
//! `cairo-version-string` to `"1.18.4"` -- literals in `eval.rs` naming the
//! GTK and cairo that this machine's GNU happens to be built against, in a
//! port whose display stack is winit + wgpu + WPE and which answers
//! `(featurep 'gtk)` nil.  They were the only two names in the whole sweep
//! whose every GNU declaration is coupled to a feature absent here AND that no
//! pin held; ledger 199 removed them.
//!
//! The rule also declines to flag what must stay.  `use-system-tooltips` is
//! `DEFVAR_BOOL` at `src/frame.c:7725` and `scroll-bar-adjust-thumb-portion`
//! at `src/frame.c:7465`, both outside every `#ifdef`, so `syms_of_frame`
//! binds them in a GNU tty build too -- and neither appears in this table.
//! Reading `cus-start.el` as two-way would have deleted the second one, whose
//! rule there is `(featurep 'x)` (`:918-923`), and it is as platform-neutral
//! as the first.  That is the error this module exists to make unspellable in
//! both directions.
//!
//! # What the [`HereDecision`] variants buy
//!
//! [`HereDecision::Absent`] is a claim about the obarray that
//! `provide_coupled_vars_test` checks at run time, not a comment.  There is
//! deliberately no variant meaning "bound, and nobody recorded why":
//! [`HereDecision::BoundByPolicy`] requires the entry that decided it and the
//! pin that holds it, the same shape `c_features::HereDecision` uses for the
//! `Fprovide` half (ledger 192/197).  A new GTK-only name arriving in the
//! obarray has no row and the scan fails; a row claiming `Absent` whose name
//! is bound fails; and a `BoundByPolicy` row whose name is no longer bound
//! fails too, so a policy cannot outlive the thing it excused.
//!
//! Ledger 199.

use CoupledFeature::{
    Android, Cairo, DbusBind, DynamicSetting, Gtk, Haiku, Motif, MsDos, NativeCompile, Ns, Pgtk,
    W32, X, XwidgetInternal,
};

/// The single question this build can answer that decides whether GNU's
/// declaration site could have compiled.
///
/// Every variant is an `Fprovide` in GNU's `src/*.c` -- these are features, not
/// `#ifdef` names, precisely so the scan can ask the running obarray instead of
/// a build script.  `MsDos` is the one exception GNU itself makes:
/// `src/msdos.c` and `src/dosfns.c` provide nothing, and `lisp/cus-start.el:901`
/// asks `(eq system-type 'ms-dos)` instead.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CoupledFeature {
    /// `Fprovide (Qx)`, `src/xfns.c:10498`, inside the `HAVE_X_WINDOWS` arm of
    /// `src/emacs.c:2373-2385` that runs `syms_of_xterm`, `syms_of_xfns`,
    /// `syms_of_xmenu`, `syms_of_xsettings`, `syms_of_xsmfns` and
    /// `syms_of_xselect`.
    X,
    /// `Fprovide ("gtk")`, `src/xfns.c:10540` (`#ifdef USE_GTK`) and
    /// `src/pgtkfns.c:3786`.
    Gtk,
    /// `Fprovide ("cairo")`, `src/xfns.c:10553` (`#ifdef USE_CAIRO`),
    /// `src/pgtkfns.c:3800` and `src/haikuterm.c:4886`.
    Cairo,
    /// `Fprovide ("motif")`, `src/xfns.c:10526` (`USE_X_TOOLKIT` + `USE_MOTIF`).
    Motif,
    /// `Fprovide (Qpgtk)`, `src/pgtkterm.c:7502`; `src/emacs.c:2429-2436`.
    Pgtk,
    /// `Fprovide (Qns)`, `src/nsterm.m:11744`; `src/emacs.c:2421-2426`.
    Ns,
    /// `Fprovide (Qhaiku)`, `src/haikuterm.c:4884`; `src/emacs.c:2438-2447`.
    Haiku,
    /// `Fprovide (Qw32)`, `src/w32term.c:8396`; `src/emacs.c:2399-2411`.
    W32,
    /// `Fprovide (Qandroid)`, `src/androidterm.c:6984`; `src/emacs.c:2449-2459`.
    Android,
    /// No `Fprovide` at all: `src/emacs.c:2414-2418` runs `syms_of_dosfns` and
    /// `syms_of_msdos` under `#ifdef MSDOS`, and `lisp/cus-start.el:901` asks
    /// `(eq system-type 'ms-dos)`.
    MsDos,
    /// `Fprovide ("xwidget-internal")`, `src/xwidget.c:4003` -- a feature name,
    /// never a `DEFVAR`, which is why `cus-start.el:946`'s
    /// `(boundp 'xwidget-internal)` probe answers nil in every GNU build,
    /// including one built WITH xwidgets.
    ///
    /// `src/emacs.c:2489` calls `syms_of_xwidget` unconditionally, so the guard
    /// is one level down: `src/xwidget.h:233` makes it
    /// `INLINE void syms_of_xwidget (void) {}` when `HAVE_XWIDGETS` is
    /// undefined.  The coupling is exact all the same.
    XwidgetInternal,
    /// `Fprovide ("dbusbind")`, `src/dbusbind.c:2175`; `src/emacs.c:2478`.
    DbusBind,
    /// `Fprovide ("native-compile")`, `src/comp.c:5825`.
    NativeCompile,
    /// `Fprovide (Qdynamic_setting)`, `src/xsettings.c:1417` and
    /// `src/haikufont.c:1479` -- unconditional inside a `syms_of_*` that only
    /// an X, PGTK or Haiku build runs (`src/emacs.c:2378`, `:2436`, `:2442`).
    DynamicSetting,
}

/// Why a name coupled to an absent feature is, or is not, on this obarray.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HereDecision {
    /// Not bound here either -- GNU's coupling is honoured.
    Absent,
    /// Bound here anyway.  `policy` names the entry that decided that and the
    /// pin that holds it; the fact cannot be recorded without one.
    BoundByPolicy { policy: &'static str },
}

/// One GNU C variable that no build with this build's answers could bind.
#[derive(Copy, Clone, Debug)]
pub struct ProvideCoupledVariable {
    pub name: &'static str,
    /// The feature deciding each of GNU's declaration sites, deduplicated.
    pub features: &'static [CoupledFeature],
    /// Every `DEFVAR_*` GNU has for this name, `file:line`, space separated.
    pub gnu: &'static str,
    pub here: HereDecision,
}

const fn absent(
    name: &'static str,
    features: &'static [CoupledFeature],
    gnu: &'static str,
) -> ProvideCoupledVariable {
    ProvideCoupledVariable {
        name,
        features,
        gnu,
        here: HereDecision::Absent,
    }
}

const fn bound_by(
    name: &'static str,
    features: &'static [CoupledFeature],
    gnu: &'static str,
    policy: &'static str,
) -> ProvideCoupledVariable {
    ProvideCoupledVariable {
        name,
        features,
        gnu,
        here: HereDecision::BoundByPolicy { policy },
    }
}

/// Ledger 189's decision, and the pin that holds it.
///
/// 189 measured GNU's whole X-only C surface and declined to remove it, because
/// `crates/neovm-oracle-tests/src/defvar_bool_byte_boolean_vars.rs`'s
/// `oracle_every_defvar_bool_variable_is_bound_and_canonical` asserts that all
/// of GNU's `DEFVAR_BOOL` names are bound AND canonical here, and it agrees
/// with GNU only because the oracle reference binary is built with X.  Removing
/// the declarations replaces that agreement with a hand-maintained exception
/// list.  `lisp/cus-start.el` pushes the same way from the other side: a
/// generic `x-` name is gated on `(fboundp 'x-create-frame)` (`:924`) and a
/// name containing "selection" on `(fboundp 'x-selection-exists-p)` (`:926`),
/// both of which this build answers `t` -- so `cus-start` would ERROR on the
/// very absence it would otherwise be justifying.  That contradiction is
/// ledger 179/189's open structural item, not this table's to settle.
const X_SURFACE: &str = "ledger 189: the X C surface is pinned against GNU+X by neovm-oracle-tests \
     defvar_bool_byte_boolean_vars.rs oracle_every_defvar_bool_variable_is_bound_and_canonical, \
     and lisp/cus-start.el:924,926 gates these on (fboundp 'x-create-frame) / \
     (fboundp 'x-selection-exists-p), which this build answers t";

/// This port ships a real xwidget layer, so these variables are backed.
///
/// The row is now platform-split, and `c_features.rs` is still the only thing
/// that decides it.  On a macOS build with the `webview` feature
/// `xwidget-internal` IS provided -- `crates/neomacs-webview/src/platform/macos`
/// places a native `WKWebView`, and `xwidget-webkit-browse-url` works in every
/// top-level frame -- so these three rows describe a build that has the
/// feature, and binding the variables matches GNU exactly rather than
/// excusing a gap.  Issue 300 tracks the remaining divergences.  On Linux the
/// WPE path still leaves ledger 190's missing subrs open, the feature is not
/// provided, and the rows remain policy.
const XWIDGET_LAYER: &str = "ledger 190/192: xwidget.rs implements GNU's xwidget subrs over WPE/WebKit \
     (Linux, xwidget-internal NotBuilt) or a native WKWebView (macOS with the `webview' \
     feature, provided)";

/// `font-use-system-font` is `(car byte-boolean-vars)` in both engines, pinned
/// by `crates/neovm-oracle-tests/src/defvar_bool_byte_boolean_vars.rs:42`
/// (`"OK (117 font-use-system-font ...)"`), and `xft-settings` is pinned bound
/// with its docstring at
/// `crates/neovm-oracle-tests/src/snarf_documentation_boundp_clause.rs:116`.  Both
/// pins compare against a GNU built with X, where `syms_of_xsettings` runs.
/// `lisp/cus-start.el:932` independently says the opposite -- it gates
/// `font-use-system-font` on `(featurep 'system-font-setting)`, which is nil
/// here -- so this is a row where GNU's two authorities agree with each other
/// and only the oracle reference disagrees.
const XSETTINGS_ORACLE_PIN: &str = "ledger 199: font-use-system-font is (car byte-boolean-vars) at \
     defvar_bool_byte_boolean_vars.rs:42 and xft-settings is pinned bound at \
     snarf_documentation_boundp_clause.rs:116, both against a GNU built with X";

/// Every GNU `src/*.c` `DEFVAR_*` name whose declaration sites are ALL decided
/// by a feature this build does not have.
///
/// Derived mechanically from GNU 31.0.90 (`0ee48ac4df2`): 928 `DEFVAR_*` names
/// across `src/*.c` and `src/*.m`, each site's enclosing `#if`/`#ifdef` stack
/// matched against the `Fprovide` calls in the same block, and the file-level
/// guard taken from `src/emacs.c`'s own `syms_of_*` dispatch.  234 names
/// survive that filter; this build binds 74 of them, each with a policy.
/// Kept one row per line, like `defvar_object::gnu_table` and
/// `var_docs::gnu_table`: the rows are generated, and a row that reads as one
/// line diffs as one line when the derivation is re-run.
#[rustfmt::skip]
pub static PROVIDE_COUPLED_VARIABLES: &[ProvideCoupledVariable] = &[
    absent("android-build-fingerprint", &[Android], "androidterm.c:7026"),
    absent("android-build-manufacturer", &[Android], "androidterm.c:7032"),
    absent("android-display-planes", &[Android], "androidterm.c:7036"),
    absent("android-intercept-control-space", &[Android], "androidfns.c:3688"),
    absent("android-keyboard-bell-duration", &[Android], "androidfns.c:3715"),
    absent("android-os-language", &[Android], "androidfns.c:3723"),
    absent("android-pass-multimedia-buttons-to-system", &[Android], "androidfns.c:3677"),
    absent("android-quit-keycode", &[Android], "androidterm.c:6998"),
    absent("android-use-exec-loader", &[Android], "androidfns.c:3701"),
    absent("android-wait-for-event-timeout", &[Android], "androidterm.c:6986"),
    absent("cairo-version-string", &[Cairo, Haiku], "haikufns.c:3312 pgtkfns.c:3802 xfns.c:10555"),
    absent("comp--#$", &[NativeCompile], "comp.c:5821"),
    absent("comp-abi-hash", &[NativeCompile], "comp.c:5726"),
    absent("comp-ctxt", &[NativeCompile], "comp.c:5718"),
    absent("comp-deferred-pending-h", &[NativeCompile], "comp.c:5733"),
    absent("comp-eln-to-el-h", &[NativeCompile], "comp.c:5738"),
    absent("comp-file-preloaded-p", &[NativeCompile], "comp.c:5797"),
    absent("comp-installed-trampolines-h", &[NativeCompile], "comp.c:5784"),
    absent("comp-loaded-comp-units-h", &[NativeCompile], "comp.c:5800"),
    absent("comp-native-version-dir", &[NativeCompile], "comp.c:5729"),
    absent("comp-no-native-file-h", &[NativeCompile], "comp.c:5790"),
    absent("comp-sanitizer-active", &[NativeCompile], "comp.c:5813"),
    absent("comp-subr-arities-h", &[NativeCompile], "comp.c:5805"),
    absent("comp-subr-list", &[NativeCompile], "comp.c:5724"),
    absent("dbus-compiled-version", &[DbusBind], "dbusbind.c:2069"),
    absent("dbus-debug", &[DbusBind], "dbusbind.c:2159"),
    absent("dbus-message-type-error", &[DbusBind], "dbusbind.c:2108"),
    absent("dbus-message-type-invalid", &[DbusBind], "dbusbind.c:2092"),
    absent("dbus-message-type-method-call", &[DbusBind], "dbusbind.c:2097"),
    absent("dbus-message-type-method-return", &[DbusBind], "dbusbind.c:2102"),
    absent("dbus-message-type-signal", &[DbusBind], "dbusbind.c:2113"),
    absent("dbus-registered-objects-table", &[DbusBind], "dbusbind.c:2118"),
    absent("dbus-runtime-version", &[DbusBind], "dbusbind.c:2078"),
    absent("dos-codepage", &[MsDos], "dosfns.c:719"),
    absent("dos-country-code", &[MsDos], "dosfns.c:715"),
    absent("dos-decimal-point", &[MsDos], "dosfns.c:790"),
    absent("dos-display-scancodes", &[MsDos], "dosfns.c:743"),
    absent("dos-hyper-key", &[MsDos], "dosfns.c:751"),
    absent("dos-keyboard-layout", &[MsDos], "dosfns.c:785"),
    absent("dos-keypad-mode", &[MsDos], "dosfns.c:761"),
    absent("dos-super-key", &[MsDos], "dosfns.c:756"),
    absent("dos-timezone-offset", &[MsDos], "dosfns.c:731"),
    absent("dos-unsupported-char-glyph", &[MsDos], "msdos.c:4331"),
    absent("dos-version", &[MsDos], "dosfns.c:735"),
    absent("dos-windows-version", &[MsDos], "dosfns.c:739"),
    bound_by("font-use-system-font", &[DynamicSetting], "haikufont.c:1459 xsettings.c:1395", XSETTINGS_ORACLE_PIN),
    absent("gtk-version-string", &[Cairo, Gtk], "pgtkfns.c:3788 xfns.c:10542"),
    absent("haiku-allowed-ui-colors", &[Haiku], "haikufns.c:3305"),
    absent("haiku-control-keysym", &[Haiku], "haikuterm.c:4852"),
    absent("haiku-debug-on-fatal-error", &[Haiku], "haikuterm.c:4833"),
    absent("haiku-drag-track-function", &[Haiku], "haikuselect.c:1381"),
    absent("haiku-drag-wheel-function", &[Haiku], "haikuselect.c:1392"),
    absent("haiku-initialized", &[Haiku], "haikuterm.c:4816"),
    absent("haiku-lost-selection-functions", &[Haiku], "haikuselect.c:1387"),
    absent("haiku-meta-keysym", &[Haiku], "haikuterm.c:4844"),
    absent("haiku-pass-control-tab-to-system", &[Haiku], "haikufns.c:3297"),
    absent("haiku-shift-keysym", &[Haiku], "haikuterm.c:4868"),
    absent("haiku-signal-invalid-refs", &[Haiku], "haikuselect.c:1375"),
    absent("haiku-super-keysym", &[Haiku], "haikuterm.c:4860"),
    absent("motif-version-string", &[Motif], "xfns.c:10528"),
    absent("native-comp-eln-load-path", &[NativeCompile], "comp.c:5742"),
    absent("native-comp-enable-subr-trampolines", &[NativeCompile], "comp.c:5759"),
    absent("native-comp-jit-compilation", &[NativeCompile], "comp.c:5569"),
    absent("ns-alternate-modifier", &[Ns], "nsterm.m:11563"),
    absent("ns-antialias-text", &[Ns], "nsterm.m:11640"),
    absent("ns-auto-hide-menu-bar", &[Ns], "nsterm.m:11652"),
    absent("ns-click-through", &[Ns], "nsterm.m:11715"),
    absent("ns-command-modifier", &[Ns], "nsterm.m:11584"),
    absent("ns-confirm-quit", &[Ns], "nsterm.m:11648"),
    absent("ns-control-modifier", &[Ns], "nsterm.m:11609"),
    absent("ns-drag-motion-function", &[Ns], "nsterm.m:11733"),
    absent("ns-function-modifier", &[Ns], "nsterm.m:11630"),
    absent("ns-icon-type-alist", &[Ns], "nsfns.m:4208"),
    absent("ns-input-file", &[Ns], "nsterm.m:11555"),
    absent("ns-input-font", &[Ns], "nsterm.m:11535"),
    absent("ns-input-fontsize", &[Ns], "nsterm.m:11539"),
    absent("ns-input-line", &[Ns], "nsterm.m:11543"),
    absent("ns-input-spi-arg", &[Ns], "nsterm.m:11551"),
    absent("ns-input-spi-name", &[Ns], "nsterm.m:11547"),
    absent("ns-mwheel-line-height", &[Ns], "nsterm.m:11683"),
    absent("ns-reg-to-script", &[Ns], "nsfont.m:1749"),
    absent("ns-right-alternate-modifier", &[Ns], "nsterm.m:11573"),
    absent("ns-right-command-modifier", &[Ns], "nsterm.m:11598"),
    absent("ns-right-control-modifier", &[Ns], "nsterm.m:11619"),
    absent("ns-scroll-event-delta-factor", &[Ns], "nsterm.m:11727"),
    absent("ns-sent-selection-hooks", &[Ns], "nsselect.m:812"),
    absent("ns-use-fullscreen-animation", &[Ns], "nsterm.m:11665"),
    absent("ns-use-mwheel-acceleration", &[Ns], "nsterm.m:11677"),
    absent("ns-use-mwheel-momentum", &[Ns], "nsterm.m:11689"),
    absent("ns-use-native-fullscreen", &[Ns], "nsterm.m:11657"),
    absent("ns-use-proxy-icon", &[Ns], "nsfns.m:4233"),
    absent("ns-use-srgb-colorspace", &[Ns], "nsterm.m:11671"),
    absent("ns-use-thin-smoothing", &[Ns], "nsterm.m:11644"),
    absent("ns-version-string", &[Ns], "nsfns.m:4229"),
    absent("ns-working-text", &[Ns], "nsterm.m:11559"),
    absent("pgtk-keysym-table", &[Pgtk], "pgtkterm.c:7494"),
    absent("pgtk-lost-selection-functions", &[Pgtk], "pgtkselect.c:1912"),
    absent("pgtk-selection-alias-alist", &[Pgtk], "pgtkselect.c:1954"),
    absent("pgtk-selection-timeout", &[Pgtk], "pgtkselect.c:1948"),
    absent("pgtk-sent-selection-functions", &[Pgtk], "pgtkselect.c:1920"),
    absent("pgtk-sent-selection-hooks", &[Pgtk], "pgtkselect.c:1934"),
    absent("pgtk-use-im-context-on-new-connection", &[Pgtk], "pgtkim.c:306"),
    absent("pgtk-wait-for-event-timeout", &[Pgtk], "pgtkterm.c:7483"),
    bound_by("selection-converter-alist", &[Pgtk, X], "pgtkselect.c:1908 xselect.c:3374", X_SURFACE),
    absent("sfnt-default-family-alist", &[Android], "sfntfont.c:4171"),
    absent("sfnt-raster-glyphs-exactly", &[Android], "sfntfont.c:4194"),
    absent("sfnt-uninstructable-family-regexp", &[Android], "sfntfont.c:4179"),
    absent("w32--terminal-is-conhost", &[W32], "w32console.c:1236"),
    absent("w32-add-wrapped-menu-bar-lines", &[W32], "w32term.c:8384"),
    absent("w32-alt-is-meta", &[W32], "w32fns.c:11638"),
    absent("w32-ansi-code-page", &[W32], "w32fns.c:12389"),
    absent("w32-apps-modifier", &[W32], "w32fns.c:11742"),
    absent("w32-capslock-is-shiftlock", &[W32], "w32term.c:8293"),
    absent("w32-charset-info-alist", &[W32], "w32font.c:3010"),
    absent("w32-collate-ignore-punctuation", &[W32], "w32proc.c:4855"),
    absent("w32-color-map", &[W32], "w32fns.c:11627"),
    absent("w32-disable-abort-dialog", &[W32], "w32fns.c:11894"),
    absent("w32-disable-double-buffering", &[W32], "w32fns.c:12403"),
    absent("w32-disable-new-uniscribe-apis", &[W32], "w32fns.c:11872"),
    absent("w32-downcase-file-names", &[W32], "w32proc.c:4822"),
    absent("w32-enable-caps-lock", &[W32], "w32fns.c:11707"),
    absent("w32-enable-num-lock", &[W32], "w32fns.c:11701"),
    absent("w32-enable-palette", &[W32], "w32fns.c:11754"),
    absent("w32-enable-synthesized-fonts", &[W32], "w32fns.c:11750"),
    absent("w32-follow-system-dark-mode", &[W32], "w32fns.c:12409"),
    absent("w32-generate-fake-inodes", &[W32], "w32proc.c:4831"),
    absent("w32-get-true-file-attributes", &[W32], "w32proc.c:4840"),
    absent("w32-grab-focus-on-raise", &[W32], "w32term.c:8285"),
    absent("w32-ignore-modifiers-on-IME-input", &[W32], "w32fns.c:11899"),
    absent("w32-inhibit-dwrite", &[W32], "w32dwrite.c:1356"),
    absent("w32-lwindow-modifier", &[W32], "w32fns.c:11722"),
    absent("w32-mouse-button-tolerance", &[W32], "w32fns.c:11758"),
    absent("w32-mouse-move-interval", &[W32], "w32fns.c:11767"),
    absent("w32-multibyte-code-page", &[W32], "w32fns.c:12395"),
    absent("w32-num-mouse-buttons", &[W32], "w32term.c:8274"),
    absent("w32-pass-alt-to-system", &[W32], "w32fns.c:11631"),
    absent("w32-pass-extra-mouse-buttons-to-system", &[W32], "w32fns.c:11775"),
    absent("w32-pass-lwindow-to-system", &[W32], "w32fns.c:11647"),
    absent("w32-pass-multimedia-buttons-to-system", &[W32], "w32fns.c:11785"),
    absent("w32-pass-rwindow-to-system", &[W32], "w32fns.c:11666"),
    absent("w32-phantom-key-code", &[W32], "w32fns.c:11685"),
    absent("w32-pipe-buffer-size", &[W32], "w32proc.c:4815"),
    absent("w32-pipe-read-delay", &[W32], "w32proc.c:4801"),
    absent("w32-quit-key", &[W32], "w32fns.c:11643"),
    absent("w32-quote-process-args", &[W32], "w32proc.c:4765"),
    absent("w32-recognize-altgr", &[W32], "w32term.c:8299"),
    absent("w32-rwindow-modifier", &[W32], "w32fns.c:11732"),
    absent("w32-scroll-lock-modifier", &[W32], "w32fns.c:11713"),
    absent("w32-start-process-inherit-error-mode", &[W32], "w32proc.c:4794"),
    absent("w32-start-process-share-console", &[W32], "w32proc.c:4784"),
    absent("w32-start-process-show-window", &[W32], "w32proc.c:4777"),
    absent("w32-strict-painting", &[W32], "w32fns.c:11856"),
    absent("w32-swap-mouse-buttons", &[W32], "w32term.c:8279"),
    absent("w32-tooltip-extra-pixels", &[W32], "w32fns.c:11882"),
    absent("w32-unicode-filenames", &[W32], "w32term.c:8340"),
    absent("w32-use-fallback-wm-chars-method", &[W32], "w32fns.c:11863"),
    absent("w32-use-full-screen-buffer", &[W32], "w32console.c:1227"),
    absent("w32-use-native-image-API", &[W32], "w32term.c:8355"),
    absent("w32-use-visible-system-caret", &[W32], "w32term.c:8306"),
    absent("w32-yes-no-dialog-show-cancel", &[W32], "w32term.c:8376"),
    bound_by("x-allow-focus-stealing", &[X], "xterm.c:33013", X_SURFACE),
    bound_by("x-alt-keysym", &[Android, Pgtk, X], "androidterm.c:7051 pgtkterm.c:7453 xterm.c:32763", X_SURFACE),
    bound_by("x-auto-preserve-selections", &[X], "xterm.c:32976", X_SURFACE),
    bound_by("x-color-cache-bucket-size", &[X], "xterm.c:32922", X_SURFACE),
    bound_by("x-ctrl-keysym", &[Android, Pgtk, X], "androidterm.c:7047 pgtkterm.c:7449 xterm.c:32755", X_SURFACE),
    bound_by("x-cursor-fore-pixel", &[Android, Cairo, Haiku, W32, X], "androidfns.c:3662 haikufns.c:3280 pgtkfns.c:3782 w32fns.c:11837 xfns.c:10432", X_SURFACE),
    bound_by("x-detect-server-trust", &[X], "xterm.c:33054", X_SURFACE),
    bound_by("x-dnd-disable-motif-drag", &[X], "xterm.c:32878", X_SURFACE),
    bound_by("x-dnd-disable-motif-protocol", &[X], "xterm.c:32954", X_SURFACE),
    bound_by("x-dnd-fix-motif-leave", &[X], "xterm.c:32870", X_SURFACE),
    bound_by("x-dnd-movement-function", &[X], "xterm.c:32885", X_SURFACE),
    bound_by("x-dnd-native-test-function", &[X], "xterm.c:32934", X_SURFACE),
    bound_by("x-dnd-preserve-selection-data", &[X], "xterm.c:32947", X_SURFACE),
    bound_by("x-dnd-targets-list", &[X], "xterm.c:32927", X_SURFACE),
    bound_by("x-dnd-unsupported-drop-function", &[X], "xterm.c:32901", X_SURFACE),
    bound_by("x-dnd-use-unsupported-drop", &[X], "xterm.c:32960", X_SURFACE),
    bound_by("x-dnd-wheel-function", &[X], "xterm.c:32892", X_SURFACE),
    bound_by("x-fast-protocol-requests", &[X], "xterm.c:32967", X_SURFACE),
    bound_by("x-fast-selection-list", &[X], "xterm.c:33000", X_SURFACE),
    bound_by("x-frame-normalize-before-maximize", &[X], "xterm.c:32812", X_SURFACE),
    bound_by("x-gtk-file-dialog-help-text", &[Cairo, X], "pgtkfns.c:3880 xfns.c:10473", X_SURFACE),
    bound_by("x-gtk-resize-child-frames", &[X], "xfns.c:10479", X_SURFACE),
    bound_by("x-gtk-show-hidden-files", &[Cairo, X], "pgtkfns.c:3876 xfns.c:10467", X_SURFACE),
    bound_by("x-gtk-use-native-input", &[X], "xterm.c:32839", X_SURFACE),
    bound_by("x-gtk-use-old-file-dialog", &[Cairo, X], "pgtkfns.c:3872 xfns.c:10460", X_SURFACE),
    bound_by("x-gtk-use-window-move", &[X], "xterm.c:32825", X_SURFACE),
    bound_by("x-hourglass-pointer-shape", &[Android, Haiku, W32, X], "androidfns.c:3597 haikufns.c:3288 w32fns.c:11817 xfns.c:10341", X_SURFACE),
    bound_by("x-hyper-keysym", &[Android, Pgtk, X], "androidterm.c:7055 pgtkterm.c:7457 xterm.c:32772", X_SURFACE),
    bound_by("x-input-coding-function", &[X], "xterm.c:32993", X_SURFACE),
    bound_by("x-input-coding-system", &[X], "xterm.c:32986", X_SURFACE),
    bound_by("x-input-grab-touch-events", &[X], "xterm.c:32860", X_SURFACE),
    bound_by("x-keysym-table", &[X], "xterm.c:32808", X_SURFACE),
    bound_by("x-lax-frame-positioning", &[X], "xterm.c:33064", X_SURFACE),
    bound_by("x-lost-selection-functions", &[X], "xselect.c:3397", X_SURFACE),
    bound_by("x-max-tooltip-size", &[Android, Cairo, Haiku, Ns, W32, X], "androidfns.c:3673 haikufns.c:3276 nsfns.m:4243 pgtkfns.c:3884 w32fns.c:11841 xfns.c:10436", X_SURFACE),
    bound_by("x-meta-keysym", &[Android, Pgtk, X], "androidterm.c:7059 pgtkterm.c:7461 xterm.c:32780", X_SURFACE),
    absent("x-mode-pointer-shape", &[Android, X], "androidfns.c:3652 xfns.c:10348"),
    bound_by("x-mouse-click-focus-ignore-position", &[X], "xterm.c:32689", X_SURFACE),
    bound_by("x-mouse-click-focus-ignore-time", &[X], "xterm.c:32704", X_SURFACE),
    bound_by("x-no-window-manager", &[W32, X], "w32fns.c:11845 xfns.c:10441", X_SURFACE),
    absent("x-nontext-pointer-shape", &[Android, X], "androidfns.c:3592 xfns.c:10334"),
    bound_by("x-pixel-size-width-font-regexp", &[W32, X], "w32fns.c:11851 xfns.c:10449", X_SURFACE),
    bound_by("x-pointer-shape", &[Android, Haiku, W32, X], "androidfns.c:3587 haikufns.c:3284 w32fns.c:11809 xfns.c:10327", X_SURFACE),
    bound_by("x-quit-keysym", &[X], "xterm.c:33076", X_SURFACE),
    bound_by("x-scroll-event-delta-factor", &[X], "xterm.c:32833", X_SURFACE),
    bound_by("x-select-enable-clipboard-manager", &[X], "xselect.c:3419", X_SURFACE),
    bound_by("x-selection-alias-alist", &[X], "xselect.c:3442", X_SURFACE),
    bound_by("x-selection-timeout", &[X], "xselect.c:3427", X_SURFACE),
    bound_by("x-sensitive-text-pointer-shape", &[Android, Haiku, W32, X], "androidfns.c:3601 haikufns.c:3292 w32fns.c:11821 xfns.c:10355", X_SURFACE),
    bound_by("x-sent-selection-functions", &[X], "xselect.c:3405", X_SURFACE),
    bound_by("x-session-id", &[X], "xsmfns.c:547", X_SURFACE),
    bound_by("x-session-previous-id", &[X], "xsmfns.c:555", X_SURFACE),
    bound_by("x-set-frame-visibility-more-laxly", &[X], "xterm.c:32845", X_SURFACE),
    bound_by("x-super-keysym", &[Android, Pgtk, X], "androidterm.c:7063 pgtkterm.c:7465 xterm.c:32789", X_SURFACE),
    bound_by("x-toolkit-scroll-bars", &[Android, Haiku, Ns, Pgtk, W32, X], "androidterm.c:7068 haikuterm.c:4829 nsterm.m:11695 pgtkterm.c:7479 w32term.c:8336 xterm.c:32711", X_SURFACE),
    bound_by("x-treat-local-requests-remotely", &[X], "xselect.c:3434", X_SURFACE),
    bound_by("x-underline-at-descent-line", &[Android, Haiku, Ns, Pgtk, W32, X], "androidterm.c:7021 haikuterm.c:4824 nsterm.m:11706 pgtkterm.c:7474 w32term.c:8330 xterm.c:32678", X_SURFACE),
    bound_by("x-use-fast-mouse-position", &[X], "xterm.c:33039", X_SURFACE),
    bound_by("x-use-underline-position-properties", &[Android, Haiku, Ns, Pgtk, W32, X], "androidterm.c:7014 haikuterm.c:4819 nsterm.m:11699 pgtkterm.c:7469 w32term.c:8323 xterm.c:32667", X_SURFACE),
    bound_by("x-wait-for-event-timeout", &[W32, X], "w32term.c:8270 xterm.c:32797", X_SURFACE),
    bound_by("x-window-bottom-edge-cursor", &[Android, X], "androidfns.c:3646 xfns.c:10418", X_SURFACE),
    bound_by("x-window-bottom-left-corner-cursor", &[Android, X], "androidfns.c:3657 xfns.c:10425", X_SURFACE),
    bound_by("x-window-bottom-right-corner-cursor", &[Android, X], "androidfns.c:3641 xfns.c:10411", X_SURFACE),
    bound_by("x-window-horizontal-drag-cursor", &[Android, W32, X], "androidfns.c:3606 w32fns.c:11826 xfns.c:10362", X_SURFACE),
    bound_by("x-window-left-edge-cursor", &[Android, X], "androidfns.c:3616 xfns.c:10376", X_SURFACE),
    bound_by("x-window-right-edge-cursor", &[Android, X], "androidfns.c:3636 xfns.c:10404", X_SURFACE),
    bound_by("x-window-top-edge-cursor", &[Android, X], "androidfns.c:3626 xfns.c:10390", X_SURFACE),
    bound_by("x-window-top-left-corner-cursor", &[Android, X], "androidfns.c:3621 xfns.c:10383", X_SURFACE),
    bound_by("x-window-top-right-corner-cursor", &[Android, X], "androidfns.c:3631 xfns.c:10397", X_SURFACE),
    bound_by("x-window-vertical-drag-cursor", &[Android, W32, X], "androidfns.c:3611 w32fns.c:11831 xfns.c:10369", X_SURFACE),
    bound_by("xft-settings", &[DynamicSetting], "xsettings.c:1402", XSETTINGS_ORACLE_PIN),
    bound_by("xwidget-list", &[XwidgetInternal], "xwidget.c:3988", XWIDGET_LAYER),
    bound_by("xwidget-view-list", &[XwidgetInternal], "xwidget.c:3992", XWIDGET_LAYER),
    bound_by("xwidget-webkit-disable-javascript", &[XwidgetInternal], "xwidget.c:3996", XWIDGET_LAYER),
];

/// The row for `name`, if GNU's coupling makes it impossible in this build.
pub fn lookup(name: &str) -> Option<&'static ProvideCoupledVariable> {
    PROVIDE_COUPLED_VARIABLES.iter().find(|v| v.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the table, checked without booting a runtime.
    #[test]
    fn every_row_names_a_feature_and_cites_gnu() {
        assert_eq!(PROVIDE_COUPLED_VARIABLES.len(), 234);
        for var in PROVIDE_COUPLED_VARIABLES {
            assert!(!var.name.is_empty());
            assert!(
                !var.features.is_empty(),
                "{} has no coupled feature, so nothing decides it",
                var.name
            );
            assert!(
                var.gnu.contains(".c:") || var.gnu.contains(".m:"),
                "{} does not cite a GNU DEFVAR site: {:?}",
                var.name,
                var.gnu
            );
            if let HereDecision::BoundByPolicy { policy } = var.here {
                assert!(
                    policy.starts_with("ledger "),
                    "{} is bound by a policy that names no entry: {:?}",
                    var.name,
                    policy
                );
            }
        }
    }

    #[test]
    fn the_table_has_no_duplicate_rows() {
        let mut names: Vec<&str> = PROVIDE_COUPLED_VARIABLES.iter().map(|v| v.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate provide-coupled row");
    }

    /// The counts the module doc states, so the prose cannot drift from the
    /// table.
    #[test]
    fn seventy_four_of_the_rows_are_bound_by_a_named_policy() {
        let bound = PROVIDE_COUPLED_VARIABLES
            .iter()
            .filter(|v| matches!(v.here, HereDecision::BoundByPolicy { .. }))
            .count();
        assert_eq!(bound, 74);
        assert_eq!(PROVIDE_COUPLED_VARIABLES.len() - bound, 160);
    }

    /// The two names ledger 199 removed, and the two it deliberately did not.
    ///
    /// `use-system-tooltips` and `scroll-bar-adjust-thumb-portion` are
    /// `DEFVAR_BOOL` in `src/frame.c` outside every `#ifdef`, so no coupling
    /// reaches them and they must have no row at all.  A row appearing for
    /// either one would mean the derivation had started deleting GNU's
    /// platform-neutral surface.
    #[test]
    fn the_platform_neutral_names_have_no_row() {
        assert!(lookup("use-system-tooltips").is_none());
        assert!(lookup("scroll-bar-adjust-thumb-portion").is_none());
        assert_eq!(
            lookup("gtk-version-string").map(|v| v.here),
            Some(HereDecision::Absent)
        );
        assert_eq!(
            lookup("cairo-version-string").map(|v| v.here),
            Some(HereDecision::Absent)
        );
    }
}
