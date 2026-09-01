//! Standing check: which names does THIS build declare as primitive subrs, and
//! the reference GNU build does not?
//!
//! This is DIVERGENCES.md 138's question -- *which build declares which name* --
//! asked of FUNCTIONS.  138 asked it of variables and answered it with
//! `cus_start_platform_vars.rs`, a table whose every row carries a measured
//! `GnuBinding`.  179 asked it of a window system's Lisp variables and answered
//! it with a `mapatoms` prefix scan in `window_system_preload_test.rs`.  This
//! file is the function-side third.
//!
//! ## Why a name may legitimately be here
//!
//! In GNU a subr's existence is a consequence of a C declaration standing
//! inside a `syms_of_*` that a build's `#ifdef` either compiles or does not, so
//! two GNU builds of the same source declare different names and neither is
//! wrong.  The reference build measured against this port is
//!
//! ```text
//! --with-native-compilation=no --with-tree-sitter --with-x-toolkit=gtk3
//! ```
//!
//! (`system-configuration-options`, GNU 31.0.90, mirror `0ee48ac4df2`), so it
//! compiles `xfns.c` and `dbusbind.c` and does NOT compile `xwidget.c`,
//! `comp.c`'s guarded body, `term.c`'s Gpm block, `ENABLE_CHECKING` or
//! `ITREE_DEBUG`.  A name this port declares because it really has the
//! capability GNU's `#ifdef` names is not a divergence; ledger 183 already
//! ruled that for the xwidget pair.
//!
//! There are exactly TWO admissible reasons, and they are the two variants of
//! [`WhyThisBuildDeclaresIt`].  Everything else this port used to declare was
//! one of three defects, and the type is shaped so that none of them can be
//! parked here as a data row (ledger 190 deleted 27 names on that reading):
//!
//! * **A capability this build does not have.**  `gpm-mouse-start` and
//!   `gpm-mouse-stop` are `#ifdef HAVE_GPM` (`src/term.c:5282-5286`) and this
//!   port links no libgpm -- and GNU's own `lisp/t-mouse.el:49` uses
//!   `(fboundp 'gpm-mouse-start)` as the test for "was Emacs built with Gpm",
//!   so declaring the name replaced GNU's "Emacs must be built with Gpm to use
//!   this mode" with a stub's "Gpm-mouse only works in the GNU/Linux console".
//!   The thirteen `comp`/`native-elisp-load` names are the same shape and
//!   bigger: GNU registers exactly ONE comp.c subr outside
//!   `#ifdef HAVE_NATIVE_COMP` -- `native-comp-available-p`
//!   (`src/comp.c:5828`, after the `#endif`) -- and this build answers `nil`
//!   to it, in agreement with the reference GNU.
//! * **A name no GNU build declares at all.**  `overlay-tree` is
//!   `#ifdef ITREE_DEBUG` (`src/buffer.c:5025`, `:6117`) and `ITREE_DEBUG` is
//!   defined nowhere in GNU's tree, so no configuration ships it; this port's
//!   was a `nil` stub.  `x-scroll-bar-foreground`, `x-scroll-bar-background`,
//!   `defining-kbd-macro-p` and `executing-kbd-macro-p` have zero occurrences
//!   anywhere in GNU's `src/` or `lisp/`.  `treesit-language-version` and
//!   `treesit-parser-changed-ranges` occur exactly ONCE each in the whole GNU
//!   tree, and it is a stale `declare-function` in GNU's own
//!   `lisp/treesit.el:72` and `:102` -- GNU renamed the first to
//!   `treesit-language-abi-version` and left the declaration behind.
//! * **A Rust reimplementation of Lisp this port already ships.**
//!   `kmacro-set-counter`, `kmacro-add-counter`, `kmacro-set-format`
//!   (`lisp/kmacro.el:285`, `:321`, `:339`) and `open-tls-stream`
//!   (`lisp/obsolete/tls.el:186`) were Rust subrs that this port's own `.el`
//!   overwrote the moment it loaded -- measured, `SUBR` before
//!   `(require 'kmacro)` and `LISP` after.  That is
//!   `rust_subrs_shadowed_by_lisp_test.rs`'s class in the half that test
//!   cannot see, because it scans only what `loadup.el` preloads and none of
//!   those four files is preloaded.
//!
//! ## Why the third test is a scan and not a list
//!
//! Ledger 173's law: a predicate over rows that exist cannot see a row that was
//! never written, so ask what the guard reports when the artifact is EMPTY.
//! Empty the table below and
//! `every_subr_gnus_c_has_no_defun_for_is_accounted_for` fails on the first
//! `neomacs-` name it meets, by name.  Green-when-empty is not reachable: the
//! measured side is a `mapatoms` over a booted runtime's obarray, which has no
//! empty state, and the table is only ever the *subtrahend*.
//!
//! `GNU_SUBR_DOCS` stands in for "the names GNU's `src/*.c` DEFUNs" because it
//! is generated from GNU's sources by a port of GNU's own `make-docfile` and
//! diffed byte-for-byte against the compiled binary (ledger 181).  It is a
//! table of *documented* DEFUNs, and a name can be a subr in BOTH editors and
//! still have no row in it -- GNU's `doc :` typo hides two real `treesit.c`
//! DEFUNs, and `lisp/subr.el`'s two `defalias`es of a subr OBJECT create two
//! function cells with no `DEFUN` of their own name at all.  Those four are
//! [`BOTH_EDITORS_DECLARE_IT`], with the mechanism per row; they are not
//! divergences and are deliberately kept out of
//! [`WhyThisBuildDeclaresIt`], which answers a different question.
//!
//! The scan below found all four on its first run, and a fifth that was a real
//! defect: **`defining-kbd-macro`.**  GNU's `lisp/help.el:356` is
//! `(fset 'defining-kbd-macro (symbol-function 'start-kbd-macro))`, commented
//! "So keyboard macro definitions are documented correctly", and this port
//! ships that exact line -- but also registered a separate Rust subr of that
//! name whose registration runs after loadup, so the `fset` was undone.
//! Measured before ledger 190 removed it:
//!
//! ```text
//! (subr-name (symbol-function 'defining-kbd-macro))   GNU "start-kbd-macro"  here "defining-kbd-macro"
//! (eq (symbol-function 'start-kbd-macro)
//!     (symbol-function 'defining-kbd-macro))          GNU t                  here nil
//! (documentation 'defining-kbd-macro)                 GNU "Record subsequent keyboard input, ..."
//!                                                                            here nil
//! ```
//!
//! -- which is the exact failure GNU's comment says the `fset` is there to
//! prevent.
//!
//! What this file deliberately does NOT decide is the `#ifdef` half: whether
//! the reference GNU compiles `xwidget.c` is a fact about a build that is not
//! in this repository.  Its guard is the oracle pin in
//! `crates/neovm-oracle-tests/src/subr_surface_build_differences.rs`, which boots both
//! editors and compares their obarrays to each other.
//!
//! Ledger 190.

use crate::emacs_core::subr_docs::gnu_table::GNU_SUBR_DOCS;
use crate::test_utils::runtime_startup_eval_one;

/// The only two reasons a name may be declared here and not by the reference
/// GNU build.
///
/// There is no variant for "a capability this build does not have", none for
/// "a name no GNU build declares" and none for "a Rust reimplementation of
/// loadable Lisp".  Re-admitting one of those means adding a variant,
/// deliberately, in a diff a reviewer reads -- which is what
/// `ShadowJustification` learnt in ledger 157 when its debt variant was deleted
/// rather than kept for the next occupant.
enum WhyThisBuildDeclaresIt {
    /// GNU has a `DEFUN` for the name, and the `#ifdef` around its `defsubr`
    /// is TRUE for a build shaped like this one and FALSE for the reference
    /// GNU.  Both fields are GNU citations, not descriptions.
    GnuDeclaresItInThisBuildsOwnBranch {
        /// The `defsubr` line in GNU's `syms_of_*` that registers it.
        gnu_defsubr: &'static str,
        /// The build condition around that line, and why it holds here.
        gnu_build_guard: &'static str,
    },
    /// GNU's `src/` has no `DEFUN` for the name anywhere: it is this port's
    /// own primitive, carried in this port's own namespace exactly as GNU
    /// carries `w32-`, `ns-`, `haiku-` and `android-` ones.  The namespace is
    /// checked, so this variant cannot be used to smuggle a bare name in.
    PortOwnPrimitiveInThePortsOwnNamespace,
}

/// A name this build declares as a primitive subr and the reference GNU build
/// does not.
struct DeclaredHere {
    name: &'static str,
    why: WhyThisBuildDeclaresIt,
}

/// A name BOTH editors declare as a primitive subr and `GNU_SUBR_DOCS` has no
/// row for.
///
/// These are not divergences -- they are not in the symmetric difference at
/// all -- and they exist only because the scan below uses a table of
/// *documented* `DEFUN`s as its stand-in for "the names GNU's `src/*.c`
/// declares.  Keeping them in their own list rather than adding a variant to
/// [`WhyThisBuildDeclaresIt`] keeps the two questions apart: that enum answers
/// "why does this build declare a name the reference GNU does not", and this
/// answers "why can the doc table not see a name both builds declare".
///
/// Each row was measured in both editors, not inferred.
struct BothEditorsDeclareItButTheDocTableCannotSeeIt {
    name: &'static str,
    /// Where the name's function cell comes from, and why no DOC record exists.
    why_no_doc_record: &'static str,
}

/// Measured 2026-08-23: five names, two mechanisms.
const BOTH_EDITORS_DECLARE_IT: &[BothEditorsDeclareItButTheDocTableCannotSeeIt] = &[
    // The row this scan was worth building for.  GNU has no `DEFUN
    // ("defining-kbd-macro", ...)` anywhere; the function cell comes from
    // `lisp/help.el:356`, which copies `start-kbd-macro`'s SUBR OBJECT so that
    // the two names share one subr and one doc string -- "So keyboard macro
    // definitions are documented correctly", says the line above it.  Before
    // ledger 190 this port ALSO registered a Rust subr of that name, after
    // loadup, which undid the `fset`.
    BothEditorsDeclareItButTheDocTableCannotSeeIt {
        name: "defining-kbd-macro",
        why_no_doc_record: "lisp/help.el:356 fsets start-kbd-macro's subr object onto it; \
                            no DEFUN of this name, so the doc comes from start-kbd-macro",
    },
    // `(defalias 'search-forward-regexp (symbol-function 're-search-forward))`
    // copies the SUBR OBJECT, not the symbol, so the alias's function cell is
    // a subr whose own name is `re-search-forward`.  Measured in both:
    // `(subr-name (symbol-function 'search-forward-regexp))` => "re-search-forward",
    // `(symbol-file ... 'defun)` => subr.elc.
    BothEditorsDeclareItButTheDocTableCannotSeeIt {
        name: "search-backward-regexp",
        why_no_doc_record: "lisp/subr.el:2287 defalias of re-search-backward's subr object; \
                            no DEFUN of this name, so no etc/DOC record either",
    },
    BothEditorsDeclareItButTheDocTableCannotSeeIt {
        name: "search-forward-regexp",
        why_no_doc_record: "lisp/subr.el:2286 defalias of re-search-forward's subr object; \
                            no DEFUN of this name, so no etc/DOC record either",
    },
    // Ledger 181: GNU spells the marker `doc :` with a space before the colon,
    // so `scan_c_stream`'s `while (c_isalpha (c))` ends the keyword scan, the
    // `/*` two characters later is never reached, and make-docfile writes no
    // record.  `emacs -Q --batch` answers nil to `(documentation ...)` for both
    // -- verified there, not inferred.  The DEFUNs are real.
    BothEditorsDeclareItButTheDocTableCannotSeeIt {
        name: "treesit-parser-tracking-line-column-p",
        why_no_doc_record: "src/treesit.c:1221 spells the marker `doc :`, so make-docfile \
                            emits no record; GNU answers nil to (documentation ...) too",
    },
    BothEditorsDeclareItButTheDocTableCannotSeeIt {
        name: "treesit-tracking-line-column-p",
        why_no_doc_record: "src/treesit.c:1203 spells the marker `doc :`, so make-docfile \
                            emits no record; GNU answers nil to (documentation ...) too",
    },
];

/// The whole set, measured 2026-08-23 against GNU 31.0.90 (`0ee48ac4df2`) and a
/// `cargo xtask fresh-build --release` binary, by taking each editor's
/// `mapatoms` list of names whose `symbol-function` satisfies
/// `subr-primitive-p` and subtracting one from the other.
///
/// 23 xwidget names + `x-load-color-file` + 39 port names = 63.
const DECLARED_HERE_AND_NOT_BY_THE_REFERENCE_GNU: &[DeclaredHere] = &[
    DeclaredHere {
        name: "delete-xwidget-view",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3956",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "get-buffer-xwidgets",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3940",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "kill-xwidget",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3975",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "make-xwidget",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3930",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "neomacs--debug-lose-device",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs--frame-snapshot",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs--heap-layout-stats",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs--record-frame-navigation-intent",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs--record-window-navigation-intent",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs--write-frame-snapshot",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-buffer-text-backend",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-clipboard-get",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-clipboard-set",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-core-backend",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-default-buffer-text-backend",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-display-monitor-attributes-list",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-effect-get",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-effect-names",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-effect-reset",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-effect-set",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-effects-apply",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-frame-edges",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-frame-geometry",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-frame-shader",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-frame-shader-set-uniform",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-image-extent",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-mouse-absolute-pixel-position",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-open-tls-stream",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-primary-selection-get",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-primary-selection-set",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-set-buffer-text-backend",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-set-default-buffer-text-backend",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-set-mouse-absolute-pixel-position",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-surface-available-p",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-surface-create",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-surface-destroy",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-surface-set-uniform",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-terminal-create",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-terminal-destroy",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-terminal-get-text",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-terminal-resize",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-terminal-set-float",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-terminal-write",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neomacs-tls-available-p",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "neovm--internal-panic",
        why: WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace,
    },
    DeclaredHere {
        name: "set-xwidget-buffer",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3966",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "set-xwidget-plist",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3960",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "set-xwidget-query-on-exit-flag",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3945",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "x-load-color-file",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xfaces.c:7583",
            gnu_build_guard: "#ifndef HAVE_X_WINDOWS -- GNU declares it in the NON-X branch, which is this build's branch",
        },
    },
    DeclaredHere {
        name: "xwidget-buffer",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3959",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-info",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3937",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-live-p",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3932",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-plist",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3958",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-query-on-exit-flag",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3944",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-resize",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3939",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-size-request",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3955",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-view-info",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3938",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-view-lookup",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3943",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-view-model",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3941",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-view-p",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3935",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-view-window",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3942",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-webkit-execute-script",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3952",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-webkit-estimated-load-progress",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3974",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-webkit-goto-uri",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3949",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-webkit-title",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3948",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidget-webkit-uri",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3947",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
    DeclaredHere {
        name: "xwidgetp",
        why: WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr: "src/xwidget.c:3931",
            gnu_build_guard: "configure.ac:4455-4507 HAVE_XWIDGETS -- the whole file is XWIDGETS_OBJ",
        },
    },
];

/// Every name this port declares in its own namespace really is in its own
/// namespace.
///
/// The `PortOwnPrimitiveInThePortsOwnNamespace` variant makes a claim a reader
/// would otherwise have to take on trust, so it is checked rather than
/// believed.  A bare GNU-shaped name -- `overlay-tree`, say -- cannot be filed
/// under it.
#[test]
fn port_namespace_rows_really_are_in_the_ports_namespace() {
    crate::test_utils::init_test_tracing();
    for row in DECLARED_HERE_AND_NOT_BY_THE_REFERENCE_GNU {
        if matches!(
            row.why,
            WhyThisBuildDeclaresIt::PortOwnPrimitiveInThePortsOwnNamespace
        ) {
            assert!(
                row.name.starts_with("neomacs-") || row.name.starts_with("neovm-"),
                "{:?} is filed as one of this port's own primitives but is not in \
                 this port's namespace; if GNU has a DEFUN for it, cite the defsubr \
                 with GnuDeclaresItInThisBuildsOwnBranch instead",
                row.name
            );
        }
    }
    assert!(
        !DECLARED_HERE_AND_NOT_BY_THE_REFERENCE_GNU.is_empty(),
        "the table is empty; this build declares 39 primitives of its own, so an \
         empty table is a deleted table rather than a clean one"
    );
}

/// The rows filed under `GnuDeclaresItInThisBuildsOwnBranch` really do have a
/// GNU `DEFUN`.
///
/// `GNU_SUBR_DOCS` is generated from GNU's `src/*.c` by a port of GNU's own
/// `make-docfile` (ledger 181), so a row that claims a GNU citation and has no
/// GNU row is a citation nobody checked.
#[test]
fn rows_that_claim_a_gnu_defun_have_one() {
    crate::test_utils::init_test_tracing();
    for row in DECLARED_HERE_AND_NOT_BY_THE_REFERENCE_GNU {
        if let WhyThisBuildDeclaresIt::GnuDeclaresItInThisBuildsOwnBranch {
            gnu_defsubr,
            gnu_build_guard,
        } = row.why
        {
            assert!(
                GNU_SUBR_DOCS.iter().any(|(n, _)| *n == row.name),
                "{:?} cites {gnu_defsubr} ({gnu_build_guard}) but GNU's src/*.c has \
                 no documented DEFUN of that name",
                row.name
            );
        }
    }
}

/// `defining-kbd-macro` and `start-kbd-macro` share ONE subr object, as GNU's
/// `lisp/help.el:356` arranges.
///
/// This is the row the scan below found, written out as its own pin because a
/// count cannot show what went wrong.  `(fset 'defining-kbd-macro
/// (symbol-function 'start-kbd-macro))` exists so `C-h k` on a key bound to
/// `defining-kbd-macro` shows `start-kbd-macro`'s documentation -- GNU says so
/// in the comment on the line above.  A second Rust subr of the same name,
/// registered after loadup, silently replaced the `fset`'s result.
///
/// RED before ledger 190, against a `fresh-build --release` binary of
/// `79b418443`:
/// `("start-kbd-macro" "defining-kbd-macro" nil nil)` against GNU 31.0.90's
/// `("start-kbd-macro" "start-kbd-macro" t "Record subsequent keyboard input,
/// defining a keyboard macro.")` -- note the fourth element, which is
/// `(documentation 'defining-kbd-macro)` and was nil here.
#[test]
fn defining_kbd_macro_is_help_els_fset_of_start_kbd_macro() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list (subr-name (symbol-function 'start-kbd-macro))
               (subr-name (symbol-function 'defining-kbd-macro))
               (eq (symbol-function 'start-kbd-macro)
                   (symbol-function 'defining-kbd-macro))
               (car (split-string (documentation 'defining-kbd-macro) \"\\n\")))",
    );
    assert_eq!(
        result,
        "OK (\"start-kbd-macro\" \"start-kbd-macro\" t \
         \"Record subsequent keyboard input, defining a keyboard macro.\")"
    );
}

/// The five names on [`BOTH_EDITORS_DECLARE_IT`] really do have no
/// `GNU_SUBR_DOCS` row.
///
/// A row here is an exemption from the scan below, so it has to earn it: if
/// the doc table ever grows a record for one of these names -- because GNU
/// fixes its `doc :` typo, or because someone adds a `DEFUN` -- the exemption
/// is stale and the row must go, or the scan is quietly weaker than it reads.
#[test]
fn the_doc_table_really_cannot_see_the_names_exempted_from_the_scan() {
    crate::test_utils::init_test_tracing();
    for row in BOTH_EDITORS_DECLARE_IT {
        assert!(
            !GNU_SUBR_DOCS.iter().any(|(n, _)| *n == row.name),
            "{:?} is exempted from the scan on the ground {:?}, but GNU_SUBR_DOCS \
             now HAS a row for it -- delete the exemption",
            row.name,
            row.why_no_doc_record
        );
    }
}

/// Every name this build declares as a primitive subr and GNU's `src/*.c` has
/// no `DEFUN` for is accounted for in the table above, BY NAME.
///
/// This is the half that cannot go green by attrition.  The measured side is a
/// `mapatoms` over a booted runtime's obarray, so it has no empty state; the
/// table is only subtracted from it.  Emptying the table turns this test RED on
/// the first unaccounted name, which is ledger 173's law satisfied rather than
/// quoted.
///
/// It is also the half that runs without GNU: `GNU_SUBR_DOCS` ships in this
/// repository, so a name nobody has a GNU `DEFUN` for is detectable offline.
/// The `#ifdef` half -- which of GNU's own names the REFERENCE build compiles
/// -- needs both editors and lives in the oracle suite.
#[test]
fn every_subr_gnus_c_has_no_defun_for_is_accounted_for() {
    crate::test_utils::init_test_tracing();
    let measured = runtime_startup_eval_one(
        "(let (found)
           (mapatoms
            (lambda (s)
              (let ((f (and (fboundp s) (symbol-function s))))
                (if (and f (subr-primitive-p f))
                    (push (symbol-name s) found)))))
           (mapconcat #'identity (sort found #'string<) \" \"))",
    );
    let listed = measured
        .strip_prefix("OK \"")
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_else(|| panic!("mapatoms probe did not return a string: {measured}"));
    let names: Vec<&str> = listed.split_whitespace().collect();
    assert!(
        names.len() > 1000,
        "the obarray probe found only {} primitive subrs; a runtime that did not \
         boot would make every assertion below vacuous",
        names.len()
    );

    let mut unaccounted = Vec::new();
    for name in &names {
        if GNU_SUBR_DOCS.iter().any(|(n, _)| n == name) {
            continue;
        }
        if DECLARED_HERE_AND_NOT_BY_THE_REFERENCE_GNU
            .iter()
            .any(|row| row.name == *name)
        {
            continue;
        }
        if BOTH_EDITORS_DECLARE_IT.iter().any(|row| row.name == *name) {
            continue;
        }
        unaccounted.push(*name);
    }
    assert!(
        unaccounted.is_empty(),
        "this build declares {} primitive subr(s) that GNU's src/*.c has no \
         documented DEFUN for and that nothing in \
         DECLARED_HERE_AND_NOT_BY_THE_REFERENCE_GNU accounts for: {:?}. \
         Either GNU really declares the name (cite the defsubr) or this port \
         invented it (delete it) -- see this module's header for the three \
         defects ledger 190 deleted 27 names for.",
        unaccounted.len(),
        unaccounted
    );

    let mut stale = Vec::new();
    for row in DECLARED_HERE_AND_NOT_BY_THE_REFERENCE_GNU {
        if !names.contains(&row.name) {
            stale.push(row.name);
        }
    }
    for row in BOTH_EDITORS_DECLARE_IT {
        if !names.contains(&row.name) {
            stale.push(row.name);
        }
    }
    assert!(
        stale.is_empty(),
        "the table names {} subr(s) this build does not declare any more: {:?}. \
         A row that survives its subr is a reason with nothing to explain; \
         delete the row.",
        stale.len(),
        stale
    );
}
