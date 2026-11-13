//! The `lisp/cus-start.el` names whose C declaration belongs to a platform:
//! whether GNU binds each one in a build like this one, and -- for the ones it
//! does -- which `syms_of_*` counterpart declares it here.
//!
//! `cus-start.el` lists every variable GNU's C layer can define, across all the
//! window systems and operating systems GNU builds for.  When one is not bound
//! it consults a `native-p` test -- `dos-` needs `(eq system-type 'ms-dos)`,
//! `ns-` needs `(featurep 'ns)`, `imagemagick` needs
//! `(fboundp 'imagemagick-types)`, and so on -- and only signals "built-in
//! variable `%S' not bound" when that test says this build should have had it
//! (`lisp/cus-start.el:893-951`).  So a build without MS-DOS support does not
//! need `dos-hyper-key` to exist; it needs it to NOT exist.
//!
//! Neomacs used to seed all of these to nil in one loop, which is the wrong
//! shape twice over, and entry 138 only fixed the first half.
//!
//! **Invented existence.**  Binding a name GNU leaves unbound is a divergence
//! nothing complains about: measured under GNU Emacs 31.0.90 on GNU/Linux,
//! `-Q --batch`, `(boundp 'dos-hyper-key)` is `nil` and
//! `(boundp 'imagemagick-render-type)` is `nil`, while Neomacs answered `t` to
//! both.  Entry 138 removed those 25 rows.
//!
//! **Invented defaults.**  A name GNU DOES bind here is not a stub: it has a
//! `DEFVAR_LISP` with a real initializer, and the declaration is what makes it
//! `special-variable-p` too.  A nil seed reproduces neither, so five of the
//! seven kept names were bound-but-not-declared -- `let` bound them lexically
//! where GNU binds them dynamically, and four of them held nil where GNU holds
//! a value.  Entry 141 gave each one its declaration.
//!
//! [`GnuBinding`] is that pair of measurements, and it is a required field:
//! a row cannot be added without answering "does GNU bind this in a build like
//! this one?", and neither bound variant can be written without also naming
//! the site that declares it.  **This module seeds nothing.**  There is no
//! variant that carries a value, so "bound to a placeholder GNU never has" is
//! not a state a row can express; the only way onto the obarray is a real
//! declaration at the named site, which is where GNU's default and GNU's
//! `declared_special` bit come from together.  The `UnboundHere` rows stay in
//! the table because deleting them would lose the answer -- the next author
//! would see `cus-start.el` mention `ns-antialias-text` and seed it again.
//!
//! **This table no longer gates `documentation-property`**, corrected by entry
//! 173 on 2026-08-21.  It used to, through
//! [`is_name_gnu_leaves_unbound_here`], and that was a 25-name stand-in for
//! GNU's actual rule, which is `Fboundp` at snarf time and has no table behind
//! it (`src/doc.c:606-613`).  The stand-in covered 8 of the 130 names GNU
//! leaves unbound in this build; `var_docs::SnarfedVariable` now asks the
//! question GNU asks and covers all of them, including the ones no table could
//! have listed -- `internal-interpreter-environment` is `DEFVAR_LISP`'d and
//! then uninterned three lines later (`src/eval.c:4569-4578`).  The predicate
//! is kept as the executable form of these rows' measurement, and its unit
//! test is what would notice a row drifting away from GNU.

/// Whether GNU Emacs binds a `cus-start.el` platform variable in a build like
/// this one -- GNU/Linux, X, GTK, no MS-DOS, no NS, no Haiku, no w32, no
/// xwidgets, no ImageMagick.
///
/// Measured name by name under GNU Emacs 31.0.90, `-Q --batch`, not derived
/// from the `#ifdef`s: 7 of these 32 are bound, 25 are not.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GnuBinding {
    /// GNU declares it with a `DEFVAR_*` in a `syms_of_*` this build compiles,
    /// so Neomacs must too -- with GNU's value and GNU's `declared_special`
    /// bit, which in GNU are the same statement.  `site` is the Neomacs
    /// counterpart of that `syms_of_*`, so "GNU binds this" cannot be recorded
    /// without also recording where the declaration lives.  A C declaration is
    /// in place before any Lisp runs, so these are bound in a bare `Context`.
    DeclaredInC { site: &'static str },
    /// GNU has no C declaration for it at all: preloaded Lisp installs it and
    /// `loadup.el` carries the result into the dump.  Exactly one of these
    /// names is in that shape, and it is an *alias* rather than a variable,
    /// which is why it must not be declared in Rust -- an alias inherits its
    /// target's value, docstring and forward type, and a separate variable
    /// beside `use-system-tooltips` could disagree with it.  These appear only
    /// after loadup, not in a bare `Context`.
    DeclaredInPreloadedLisp { site: &'static str },
    /// GNU leaves it unbound here, because the `DEFVAR` sits in a file this
    /// build does not compile.  Seeding it would make `boundp` disagree with
    /// GNU, and `cus-start.el`'s `native-p` test is what keeps the absence
    /// from erroring.  Nothing declares it, so it has no documentation either.
    UnboundHere,
}

/// One `cus-start.el` platform name.
#[derive(Copy, Clone, Debug)]
pub struct CusStartPlatformVariable {
    pub name: &'static str,
    /// Where GNU declares it and what it initializes it to, `file:line`.
    pub gnu: &'static str,
    /// The answer on the X11 reference build; the Linux measurement this
    /// table was written from.
    pub binding: GnuBinding,
    /// The answer on the NS reference build when it differs.  GNU compiles
    /// the nsterm.m/nsfns.m objects only under HAVE_NS and the
    /// xterm.c/xfns.c/xselect.c ones only under HAVE_X_WINDOWS, so a name
    /// that is declared on one surface cannot inherit the other's answer.
    /// `None` means both surfaces agree.
    pub(crate) ns_binding: Option<GnuBinding>,
}

const fn declared(
    name: &'static str,
    gnu: &'static str,
    site: &'static str,
) -> CusStartPlatformVariable {
    CusStartPlatformVariable {
        name,
        gnu,
        binding: GnuBinding::DeclaredInC { site },
        ns_binding: None,
    }
}

const fn declared_in_lisp(
    name: &'static str,
    gnu: &'static str,
    site: &'static str,
) -> CusStartPlatformVariable {
    CusStartPlatformVariable {
        name,
        gnu,
        binding: GnuBinding::DeclaredInPreloadedLisp { site },
        ns_binding: None,
    }
}

const fn unbound(name: &'static str, gnu: &'static str) -> CusStartPlatformVariable {
    CusStartPlatformVariable {
        name,
        gnu,
        binding: GnuBinding::UnboundHere,
        ns_binding: None,
    }
}

impl CusStartPlatformVariable {
    /// Record the NS reference build's answer when it differs from the X11
    /// one.
    pub(crate) const fn ns_binding(mut self, binding: GnuBinding) -> Self {
        self.ns_binding = Some(binding);
        self
    }

    /// The answer on `surface`; `false` selects the X11 measurement.
    /// Production visibility is crate-internal; the moved forward-module
    /// test reads it through the A' test-support widening.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn binding_on(&self, ns_surface: bool) -> GnuBinding {
        if ns_surface {
            self.ns_binding.unwrap_or(self.binding)
        } else {
            self.binding
        }
    }
}

/// GNU's NS objects declare the fifteen `ns-*` names this port now binds at
/// dump time -- eleven `DEFVAR_LISP`s from `nsterm.m:11568-11736` seeded by
/// the evaluator's NS surface module, and four `DEFVAR_BOOL`s from
/// `nsterm.m:11657-11720` / `nsfns.m:4233` carried by `GNU_BOOL_VARIABLES`.
const NS_C_SITE: &str = "eval // ns_surface.rs + defvar_bool table (NS surface)";

pub static CUS_START_PLATFORM_VARIABLES: &[CusStartPlatformVariable] = &[
    // ---- Bound under GNU/Linux + X + GTK (7) ----
    declared(
        "window-combination-limit",
        "window.c:9456 DEFVAR_LISP, Qwindow_size",
        "window_cmds::register_bootstrap_vars",
    ),
    declared(
        "void-text-area-pointer",
        "xdisp.c:38479 DEFVAR_LISP, Qarrow",
        "xdisp::register_bootstrap_vars",
    ),
    declared(
        "x-bitmap-file-path",
        "image.c:13265 DEFVAR_LISP, decode_env_path (0, PATH_BITMAPS, 0)",
        "image::register_bootstrap_vars",
    ),
    // GNU has no C declaration for this one at all: `term/x-win.el:1572' is a
    // `defvaralias' onto the `DEFVAR_BOOL' `use-system-tooltips'
    // (`frame.c:7725'), which is why setting it coerces to t/nil.  Neomacs's
    // counterpart of `term/x-win.el' in the dump is `lisp/term/neo-preload.el'.
    declared_in_lisp(
        "x-gtk-use-system-tooltips",
        "term/x-win.el:1572 defvaralias -> frame.c:7725 DEFVAR_BOOL, true",
        "lisp/term/neo-preload.el",
    )
    // loadup.el loads term/x-win.el only under (featurep 'x) (:304-309), so
    // the NS dump never installs the alias.
    .ns_binding(GnuBinding::UnboundHere),
    declared(
        "x-scroll-event-delta-factor",
        "xterm.c:32833 DEFVAR_LISP, make_float (1.0)",
        "eval.rs syms_of_xterm block",
    )
    // xterm.c is compiled only under HAVE_X_WINDOWS, so the NS reference
    // build leaves this one void.
    .ns_binding(GnuBinding::UnboundHere),
    declared(
        "x-auto-preserve-selections",
        "xterm.c:32976 DEFVAR_LISP, list2 (QCLIPBOARD, QPRIMARY)",
        "eval.rs syms_of_xterm block",
    )
    // X11-only for the same reason as x-scroll-event-delta-factor.
    .ns_binding(GnuBinding::UnboundHere),
    // The C initializer is nil; `lisp/international/fontset.el:1266' is what
    // puts the regexp there, in both editors.
    declared(
        "vertical-centering-font-regexp",
        "fontset.c:2237 DEFVAR_LISP, Qnil",
        "fontset::register_bootstrap_vars",
    ),
    // ---- Unbound under GNU/Linux (25) ----
    // src/image.c, guarded by HAVE_IMAGEMAGICK; `cus-start.el' asks
    // `(fboundp 'imagemagick-types)'.
    unbound("imagemagick-render-type", "image.c, HAVE_IMAGEMAGICK"),
    // GNU never declares a VARIABLE called `xwidget-internal' at all -- it is
    // the feature name `syms_of_xwidget' provides -- and `cus-start.el' probes
    // it with `boundp', so the answer is nil in every GNU build.
    // `xwidget-webkit-disable-javascript' is deliberately not a row here:
    // Neomacs ships a real xwidget layer (`xwidget.rs'), which is a build
    // difference from this GNU rather than an invented seed, so that module
    // declares it alongside `xwidget-list' and `xwidget-view-list'.
    unbound(
        "xwidget-internal",
        "xwidget.c, a feature name, never a DEFVAR",
    ),
    // src/nsterm.m, src/nsfns.m -- `(featurep 'ns)'.
    unbound("ns-control-modifier", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-right-control-modifier", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-command-modifier", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-right-command-modifier", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-alternate-modifier", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-right-alternate-modifier", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-function-modifier", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-antialias-text", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-auto-hide-menu-bar", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-confirm-quit", "nsfns.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-use-native-fullscreen", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-use-fullscreen-animation", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-use-srgb-colorspace", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-scroll-event-delta-factor", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    unbound("ns-click-through", "nsterm.m, HAVE_NS")
        .ns_binding(GnuBinding::DeclaredInC { site: NS_C_SITE }),
    // src/w32*.c -- `(eq system-type 'windows-nt)'.
    unbound("w32-follow-system-dark-mode", "w32fns.c, HAVE_NTGUI"),
    // src/msdos.c, src/dosfns.c -- `(eq system-type 'ms-dos)'.
    unbound("dos-display-scancodes", "msdos.c, MSDOS"),
    unbound("dos-hyper-key", "dosfns.c, MSDOS"),
    unbound("dos-super-key", "dosfns.c, MSDOS"),
    unbound("dos-keypad-mode", "dosfns.c, MSDOS"),
    unbound("dos-unsupported-char-glyph", "msdos.c, MSDOS"),
    // src/haikuterm.c, src/haikufns.c -- `(featurep 'haiku)'.
    unbound("haiku-debug-on-fatal-error", "haikuterm.c, HAVE_HAIKU"),
    unbound("haiku-use-system-tooltips", "haikufns.c, HAVE_HAIKU"),
];

/// Whether `name` is a `cus-start.el` platform name that no build reachable
/// from here declares.
///
/// GNU's documentation for a built-in variable is attached by the same
/// `DEFVAR_*` that binds it, so a name whose C file this build does not
/// compile has no `variable-documentation` either: measured under GNU,
/// `(documentation-property 'dos-hyper-key 'variable-documentation)` is `nil`.
///
/// This used to filter `var_docs::lookup`.  It no longer does: entry 173
/// replaced it with `var_docs::SnarfedVariable`, which asks this build's
/// obarray the same question GNU's `Fsnarf_documentation` asks its own
/// (`src/doc.c:606-613`) and therefore needs no list.  What remains here is
/// the executable form of the rows' measurement -- 25 names GNU leaves unbound
/// in a build like this one -- which is what the unit tests below assert
/// against and what a future author gets instead of re-deriving it.
pub fn is_name_gnu_leaves_unbound_here(name: &str) -> bool {
    let ns_surface = cfg!(target_os = "macos");
    CUS_START_PLATFORM_VARIABLES.iter().any(|var| {
        var.name == name && matches!(var.binding_on(ns_surface), GnuBinding::UnboundHere)
    })
}

#[cfg(test)]
#[path = "tests/cus_start_platform_vars_test.rs"]
mod tests;
