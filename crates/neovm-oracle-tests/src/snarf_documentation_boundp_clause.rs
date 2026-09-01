//! Oracle guards for the OTHER half of `Fsnarf_documentation`'s filter: the
//! `Fboundp` clause.
//!
//! GNU has no table of "which variables does this build have".  It has one
//! rule, applied once, at dump time (`lisp/loadup.el:476` calls
//! `Snarf-documentation`), and the rule is boundness:
//!
//! ```c
//! if ((!NILP (Fboundp (sym))
//!     || !NILP (Fmemq (sym, delayed_init)))
//!     && strncmp (end, "\nSKIP", 5))
//!   Fput (sym, Qvariable_documentation,
//!         make_fixnum (pos + end + 1 - buf));
//! ```
//!
//! (`src/doc.c:606-613`.)  The comment fifteen lines above says why
//! (`src/doc.c:585-594`): `make-docfile` is a text scanner that does not
//! evaluate the preprocessor, so `etc/DOC` is bigger than the build, and "the
//! (f)boundp checks below ensure we don't report docs for eg w32-specific
//! items on X".
//!
//! **GNU does not record an unbound name's doc differently -- it does not
//! record it at all.**  The `Fput` is the only statement in the branch, so the
//! symbol never acquires a `variable-documentation` property and
//! `documentation-property` answers nil.  Measured over the 881 names of
//! `var_docs::gnu_table` under GNU 31.0.90 `-Q --batch`: 751 bound names have
//! a doc and 130 unbound names have nil, with zero exceptions in either
//! direction.  (The `delayed_init` escape hatch is for preloaded
//! `custom-initialize-delay` defcustoms, which `custom.el:142-161` marks
//! special and deliberately leaves unbound; no C `DEFVAR` name is in it, which
//! is why the correlation is exact.)
//!
//! Neomacs has no `make-docfile` and no DOC file: `var_docs::gnu_table` is
//! generated from all of GNU's `src/*.c` and was read back for any name asked.
//! Both of `Fsnarf_documentation`'s clauses therefore had to be written down,
//! and entry 168 wrote only the `SKIP` one.
//!
//! The two pins below are the clause from its two sides.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// A name this build does not bind has no documentation.
///
/// The sample is chosen to cover every *reason* GNU leaves a `DEFVAR` name
/// unbound, because a table-shaped fix would have to enumerate them and the
/// boundness rule does not:
///
/// - `w32-quit-key`, `dos-codepage`, `haiku-control-keysym`,
///   `android-os-language`, `pgtk-keysym-table` -- the file is not compiled on
///   this platform, the case `src/doc.c:585-594` names.
/// - `comp-abi-hash`, `comp-native-version-dir`, `native-comp-eln-load-path`
///   -- `comp.c` is compiled but `syms_of_comp`'s declarations sit behind
///   `#ifdef HAVE_NATIVE_COMP` (`src/comp.c:5568` to `src/comp.c:5826`; the
///   third is declared at `src/comp.c:5742`).  The third one is here because
///   ledger 176 found a unit test asserting its doc STRING: before entry 173's
///   clause landed the port answered where GNU answers nil, and the test
///   pinned that.  Its four Lisp mentions are all valueless `(defvar ...)`
///   forms (`lisp/startup.el:520`, `lisp/subr.el:3333`,
///   `lisp/emacs-lisp/comp.el:43`, `lisp/emacs-lisp/comp-common.el:34`), which
///   make the name special without binding it.
/// - `byte-metering-on` -- `bytecode.c`, behind `#ifdef BYTE_CODE_METER`.
/// - `inhibit-try-window-id` -- `xdisp.c`, behind `#ifdef GLYPH_DEBUG`.  Its
///   sibling `inhibit-try-cursor-movement` is deliberately left out: Neomacs
///   seeds that one and GNU does not, which is a separate residual this entry
///   records rather than fixes.
/// - `internal-interpreter-environment` -- declared unconditionally in
///   `src/eval.c:4569` and then **uninterned three lines later**
///   (`Funintern (Qinternal_interpreter_environment, Qnil)`, `src/eval.c:4578`,
///   under the comment "Don't export this variable to Elisp, so no one can
///   mess with it").  No generator that reads the declaration can see this;
///   only boundness can.
#[test]
fn oracle_a_variable_this_build_does_not_bind_has_no_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (boundp s)
                (let ((doc (documentation-property s 'variable-documentation)))
                  (and (stringp doc) (car (split-string doc "\n"))))))
        '(w32-quit-key dos-codepage haiku-control-keysym android-os-language
          pgtk-keysym-table comp-abi-hash comp-native-version-dir
          native-comp-eln-load-path
          byte-metering-on inhibit-try-window-id
          internal-interpreter-environment lucid--menu-grab-keyboard
          motif-version-string sfnt-raster-glyphs-exactly debug-end-pos))"#;
    let expect = expect_test::expect![[
        r#""OK ((w32-quit-key nil nil) (dos-codepage nil nil) (haiku-control-keysym nil nil) (android-os-language nil nil) (pgtk-keysym-table nil nil) (comp-abi-hash nil nil) (comp-native-version-dir nil nil) (native-comp-eln-load-path nil nil) (byte-metering-on nil nil) (inhibit-try-window-id nil nil) (internal-interpreter-environment nil nil) (lucid--menu-grab-keyboard nil nil) (motif-version-string nil nil) (sfnt-raster-glyphs-exactly nil nil) (debug-end-pos nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// And the other side: the boundness gate is only honest if this build
/// actually declares what GNU declares, so the names GNU binds are bound here.
///
/// One name per GNU `syms_of_*` that entry 173's sweep found short, plus the
/// three that `oracle_platform_duplicated_variables_carry_the_canonical_doc_text`
/// already asserts a doc for -- those three are exactly why the clause could
/// not land alone: gating on boundness without declaring them would have
/// turned a passing doc assertion into a failing one.
#[test]
fn oracle_the_c_variables_gnu_declares_here_are_declared_here_too() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (boundp s)
                (let ((doc (documentation-property s 'variable-documentation)))
                  (and (stringp doc) (car (split-string doc "\n"))))))
        '(long-line-threshold frame-size-history internal-doc-file-name
          code-conversion-map-vector menu-prompt-more-char
          alternate-fontname-alist clone-indirect-buffer-hook
          comment-use-syntax-ppss compose-chars-after-function
          describe-bindings-check-shadowing-in-ranges display-pixels-per-inch
          text-conversion-face window-dead-windows-table x-popup-menu-function
          x-resource-class x-show-tooltip-timeout xft-settings
          x-max-tooltip-size x-no-window-manager x-wait-for-event-timeout))"#;
    let expect = expect_test::expect![[
        r#""OK ((long-line-threshold t \"Line length above which to use redisplay shortcuts.\") (frame-size-history t \"History of frame size adjustments.\") (internal-doc-file-name t \"Name of file containing documentation strings of built-in symbols.\") (code-conversion-map-vector t \"Vector of code conversion maps.\") (menu-prompt-more-char t \"Character to see next line of menu prompt.\") (alternate-fontname-alist t \"Alist of fontname vs list of the alternate fontnames.\") (clone-indirect-buffer-hook t \"Normal hook to run in the new buffer at the end of ‘make-indirect-buffer’.\") (comment-use-syntax-ppss t \"Non-nil means ‘forward-comment’ can use ‘syntax-ppss’ internally.\") (compose-chars-after-function t \"Function to adjust composition of buffer text.\") (describe-bindings-check-shadowing-in-ranges t \"If non-nil, consider command shadowing when describing ranges of keys.\") (display-pixels-per-inch t \"Pixels per inch value for non-window system displays.\") (text-conversion-face t \"Face in which to display temporary edits by an input method.\") (window-dead-windows-table t \"Hash table of dead windows.\") (x-popup-menu-function t \"Function to call to pop up a menu.\") (x-resource-class t \"The class Emacs uses to look up X resources.\") (x-show-tooltip-timeout t \"The default timeout (in seconds) for ‘x-show-tip’.\") (xft-settings t \"Font settings applied to Xft.\") (x-max-tooltip-size t \"Maximum size for tooltips.\") (x-no-window-manager t \"Non-nil if no X window manager is in use.\") (x-wait-for-event-timeout t \"How long to wait for X events.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// Ledger 178: the clause above was authoritative only for names with no
/// seeded plist entry, and these eight rode the bypass.
///
/// `eval.rs` pre-seeded a `variable-documentation` for all 1972 names of the
/// `STARTUP_VARIABLE_DOC_STUBS` and `STARTUP_VARIABLE_DOC_STRING_PROPERTIES`
/// tables, and the plist arms of `documentation_property_plan` answered before
/// the gate was reached -- the offset arm through an `or_else` that fell back
/// to the hand-typed stub *even when the gate had said no*.  So 35 unbound
/// names answered with a doc anyway.
///
/// These eight are the ones where that also diverged from GNU, and they are
/// the sharpest case available: **GNU Emacs 31 does not have these variables
/// at all.**  Not a `DEFVAR_*` in `src/*.c`, not a `defvar` in `lisp/**/*.el`,
/// not a row in `var_docs::gnu_table`; the only mentions left in the GNU tree
/// are the removal notes -- `binary-as-unsigned` at `etc/NEWS.27` ("The
/// experimental variable 'binary-as-unsigned' has been removed"),
/// `load-convert-to-unibyte` at `etc/NEWS` ("The obsolete variable ... has
/// been removed"), `display-comint-buffer-action` and
/// `display-tex-shell-buffer-action` at `etc/NEWS.30`, `redisplay-dont-pause`
/// at `etc/NEWS.24`, and `ccl-encode-ethio-font`, `font-ccl-encoder-alist` and
/// `pure-space-overflow` nowhere whatsoever.
///
/// A hand-typed doc for a variable upstream deleted is an invented default in
/// the doc's shape: there is no GNU text for it to be a copy of.
#[test]
fn oracle_a_variable_gnu_emacs_no_longer_has_is_not_documented_here() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (boundp s)
                (let ((doc (documentation-property s 'variable-documentation)))
                  (and (stringp doc) (car (split-string doc "\n"))))))
        '(binary-as-unsigned ccl-encode-ethio-font display-comint-buffer-action
          display-tex-shell-buffer-action font-ccl-encoder-alist
          load-convert-to-unibyte pure-space-overflow redisplay-dont-pause))"#;
    let expect = expect_test::expect![[
        r#""OK ((binary-as-unsigned nil nil) (ccl-encode-ethio-font nil nil) (display-comint-buffer-action nil nil) (display-tex-shell-buffer-action nil nil) (font-ccl-encoder-alist nil nil) (load-convert-to-unibyte nil nil) (pure-space-overflow nil nil) (redisplay-dont-pause nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The other three names ledger 178 measured as divergent, which the gate
/// could never have caught because all three are BOUND here.
///
/// `flex-score-match-tightness` and `hs-special-modes-alist` are the purest
/// form of the bug: GNU spells both `(defvar NAME nil)` with **no docstring**
/// (`lisp/minibuffer.el:4920`, `lisp/progmodes/hideshow.el:626`), so
/// `Finternal__define_uninitialized_variable`'s `if (!NILP (doc))`
/// (`src/eval.c:909-912`) never fires and `documentation-property` is nil in
/// GNU by construction.  The seed table supplied a sentence GNU has never
/// had.  `completion--flex-score-last-md` is a name GNU 31 deleted, kept alive
/// here by a Rust bootstrap `set_symbol_value` (`eval.rs:4661`) as well as by
/// the seed table, so it is bound here and unbound there.
#[test]
fn oracle_a_lisp_defvar_without_a_docstring_has_no_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (boundp s)
                (let ((doc (documentation-property s 'variable-documentation)))
                  (and (stringp doc) (car (split-string doc "\n"))))))
        '(flex-score-match-tightness hs-special-modes-alist))"#;
    let expect = expect_test::expect![[
        r#""OK ((flex-score-match-tightness t nil) (hs-special-modes-alist t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The extractor's own blind spot, which is the same class of bug entry 168
/// fixed and is only safe to fix once the clause above exists.
///
/// `make-docfile` skips **all** whitespace between the `doc` keyword's colon
/// and the opening `/*`: `if (c == ':') { doc_keyword = true; do c = getc
/// (infile); while (c_isspace (c)); }` (`lib-src/make-docfile.c:1148-1155`).
/// `scripts/extract_gnu_defvar_docs.py` searched for the literal `"doc: /*"`,
/// so nine `DEFVAR` blocks whose spelling is `doc:  /*`, `doc:   /*` or a
/// newline were dropped from the generated table entirely.  Three of them are
/// bound here and answered nil.
///
/// `xft-font-ascent-descent-override` is the fourth kind: restoring its row
/// would have *created* a divergence before the boundness clause existed --
/// `xftfont.c` is only compiled when `HAVE_XFT` beats `HAVE_CAIRO`
/// (`configure.ac:7228-7231`), so GNU leaves it unbound here, and a table row
/// with no gate would have answered its text.
#[test]
fn oracle_defvars_whose_doc_keyword_is_not_followed_by_exactly_one_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (boundp s)
                (let ((doc (documentation-property s 'variable-documentation)))
                  (and (stringp doc) (car (split-string doc "\n"))))))
        '(treesit-language-remap-alist
          treesit-languages-require-line-column-tracking
          treesit-major-mode-remap-alist treesit-thing-settings
          treesit-extra-load-path treesit-load-name-override-list
          inhibit-message macroexp--dynvars
          xft-font-ascent-descent-override))"#;
    let expect = expect_test::expect![[
        r#""OK ((treesit-language-remap-alist t \"An alist remapping language symbols.\") (treesit-languages-require-line-column-tracking t \"A list of languages that need line-column tracking.\") (treesit-major-mode-remap-alist t \"Alist mapping file-specified modes to ts-modes.\") (treesit-thing-settings t \"A list defining things.\") (treesit-extra-load-path t \"Additional directories to look for tree-sitter language definitions.\") (treesit-load-name-override-list t \"An override list for unconventional tree-sitter libraries.\") (inhibit-message t \"Non-nil means calls to ‘message’ are not displayed.\") (macroexp--dynvars t \"List of variables declared dynamic in the current scope.\") (xft-font-ascent-descent-override nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
