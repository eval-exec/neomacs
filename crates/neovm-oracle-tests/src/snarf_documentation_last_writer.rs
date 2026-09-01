//! Oracle guards for the ORDER `Fsnarf_documentation` runs in: it is a **last
//! writer**, not a fallback.
//!
//! `lisp/loadup.el` does these two things, 225 lines apart:
//!
//! ```elisp
//! (load "simple")                 ; :251 -- runs every defvar in the file
//! ...
//! (Snarf-documentation "DOC")     ; :476 -- and puts etc/DOC over the top
//! ```
//!
//! and the second one's write is unconditional for a name that passes the
//! gate:
//!
//! ```c
//!   if ((!NILP (Fboundp (sym)) || !NILP (Fmemq (sym, delayed_init)))
//!       && strncmp (end, "\nSKIP", 5))
//!     Fput (sym, Qvariable_documentation,
//!           make_fixnum (pos + end + 1 - buf));
//! ```
//!
//! (`src/doc.c:606-613`.)  `Fput`, not a write-if-absent -- so for any name
//! that is both a C `DEFVAR` and a preloaded Lisp `defvar` with a docstring,
//! GNU's dumped image carries **the C text**.  Measured over the 894 names of
//! `var_docs::gnu_table` in GNU 31.0.90 `-Q --batch`: 762 are bound and all
//! 762 carry an *integer* `variable-documentation`; not one carries a string
//! or a `(FILE . POS)` cons.
//!
//! Ledger 182 turned this port around to match.  It used to consult its
//! `etc/DOC` stand-in only when the plist was empty, which is a fallback and
//! therefore the opposite order.
//!
//! **The collision set has exactly one member**, and GNU's own Lisp is why:
//! `define-minor-mode` over a variable that C already defines is written with
//! `:variable`, which suppresses the macro's `defvar` --
//!
//! ```elisp
//! (define-minor-mode abbrev-mode
//!   "Toggle Abbrev mode in the current buffer. ..."
//!   ;; It's defined in C, this stops the 'define-minor-mode' macro from
//!   ;; defining it again.
//!   :variable abbrev-mode)
//! ```
//!
//! (`lisp/abbrev.el:56-63`.)  `overwrite-mode`, `transient-mark-mode`,
//! `auto-composition-mode`, `menu-bar-mode`, `tool-bar-mode` and
//! `tab-bar-mode` all do the same.  `indent-tabs-mode`
//! (`lisp/simple.el:7639`) is the one that does not, so it is the one name
//! where the order is observable -- and the reason it is nonetheless correct
//! in GNU is this entry's subject.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The one name the order is observable on, and the control group that says
/// why it is one name.
///
/// Rows are `(name integerp-of-the-plist-entry first-line-of-the-doc)`.  The
/// `integerp` column is the mechanism and the text column is the consequence:
/// an integer means `Fsnarf_documentation` wrote last, and the text that comes
/// back is `src/buffer.c`'s rather than `define-minor-mode`'s "Non-nil if
/// Indent-Tabs mode is enabled."
///
/// The integer's *value* is not pinned -- it is a byte offset into `etc/DOC`
/// in GNU and into the `var_docs` image here, so it differs between the two
/// editors and between GNU builds.  `integerp` is the part that is a fact
/// about the design.
#[test]
fn oracle_a_c_variable_a_preloaded_lisp_file_redefines_keeps_the_c_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (list s (integerp (get s 'variable-documentation))
                (let ((doc (documentation-property s 'variable-documentation t)))
                  (and (stringp doc) (car (split-string doc "\n"))))))
        '(indent-tabs-mode abbrev-mode overwrite-mode transient-mark-mode
          auto-composition-mode menu-bar-mode tool-bar-mode tab-bar-mode))"#;
    let expect = expect_test::expect![[
        r#""OK ((indent-tabs-mode t \"Indentation can insert tabs if this is non-nil.\") (abbrev-mode t \"Non-nil if Abbrev mode is enabled.\") (overwrite-mode t \"Non-nil if self-insertion should replace existing text.\") (transient-mark-mode t \"Non-nil if Transient Mark mode is enabled.\") (auto-composition-mode t \"Non-nil if Auto-Composition mode is enabled.\") (menu-bar-mode t \"Non-nil if Menu-Bar mode is enabled.\") (tool-bar-mode t \"Non-nil if Tool-Bar mode is enabled.\") (tab-bar-mode t \"Non-nil if Tab-Bar mode is enabled.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// One `mapatoms` over the whole obarray rather than N per-name pins, because
/// a per-name pin over a doc table reports green the moment the table is empty
/// (ledger 173's law, applied to this table by ledger 178).
///
/// The three counts are the diagonal GNU has none of, asked of the shipped
/// image rather than of a bare `Context`:
///
/// 1. an unbound symbol carrying a `variable-documentation` -- the state
///    `Fsnarf_documentation`'s `Fboundp` gate exists to prevent;
/// 2. an unbound symbol that `documentation-property` nonetheless answers for;
/// 3. a symbol carrying the fixnum `0`, which `src/doc.c:433-434` reserves to
///    mean "there is no doc" and `make-docfile` cannot emit.
///
/// The fourth column is the positive control the first three need: **the
/// number of symbols whose entry is an integer**, which is what
/// `Fsnarf_documentation` installs.  Zeroes are cheap to produce by not
/// writing anything, and this column is what a doc image that stopped being
/// snarfed would fail on.  It is compared with `>` rather than pinned exactly
/// because GNU's build declares 762 of these names and this one declares 766
/// (ledger 173's recorded four-name residual).
#[test]
fn oracle_the_dumped_image_documents_only_variables_it_binds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((unbound-with-entry 0) (unbound-answered 0) (reserved-zero 0) (integers 0))
  ;; A sweep that reads docs is a MUTATING instrument unless this is off:
  ;; `documentation-property' on a `(FILE . POS)' whose read fails re-LOADS
  ;; the file (`src/doc.c:311-317', `documentation-dynamic-reload' defaults to
  ;; t), and reloading re-runs its defvars.  Measured in GNU: an unguarded
  ;; sweep turns `indent-tabs-mode's snarfed integer back into simple.elc's
  ;; cons, i.e. it undoes the very write this file is about.
  (set 'documentation-dynamic-reload nil)
  (mapatoms
   (lambda (s)
     (let ((entry (get s 'variable-documentation)))
       (when entry
         (unless (boundp s) (setq unbound-with-entry (1+ unbound-with-entry)))
         (when (eq entry 0) (setq reserved-zero (1+ reserved-zero)))
         (when (integerp entry) (setq integers (1+ integers))))
       (when (and (not (boundp s))
                  (documentation-property s 'variable-documentation t))
         (setq unbound-answered (1+ unbound-answered))))))
  (list unbound-with-entry unbound-answered reserved-zero (> integers 700)))"#;
    let expect = expect_test::expect![[r#""OK (0 0 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The order in the OTHER direction, which is the half a lazy table cannot
/// express at all.
///
/// `Fsnarf_documentation` runs once, during `loadup`.  A `defvar` evaluated
/// *after* the dump is therefore a later writer than the snarf, and GNU's
/// answer is the Lisp docstring -- `src/eval.c:911` `Fput`s it over the
/// integer, and nothing re-snarfs.  So the two orderings are not a rule about
/// which SOURCE wins; they are the same rule -- last write wins -- applied at
/// two different times, and a port that decides by source rather than by time
/// must get one of the two wrong.
///
/// Measured identical in both editors before ledger 182 as well: this is the
/// direction the old code had right, and it is pinned so that making the other
/// direction right does not silently invert this one.
#[test]
fn oracle_a_defvar_evaluated_after_the_dump_outranks_the_snarfed_doc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (eval '(defvar indent-tabs-mode t "L182 runtime docstring.") t)
  (list (get 'indent-tabs-mode 'variable-documentation)
        (documentation-property 'indent-tabs-mode 'variable-documentation t)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"L182 runtime docstring.\" \"L182 runtime docstring.\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `get_doc_string`'s sanity check, which is why this port's DOC stand-in is a
/// byte image rather than a table of rows.
///
/// A fixnum on the plist is a position in `etc/DOC`, and GNU validates it by
/// walking backwards from it over the record's name to the `^_` that starts
/// the record (`src/doc.c:254-260`).  A number that does not point just past a
/// `^_V<name>\n` header answers nil.  A row index would have no invalid
/// values, so this row is the one that says the port kept GNU's shape and not
/// merely GNU's outcome.
///
/// Measured in GNU 31.0.90 `-Q --batch`, all four answer nil: `7` lands inside
/// the first record's header, `0` is `src/doc.c:433-434`'s reserved sentinel,
/// and `-1` and `12345678901` are out of range -- GNU's `error ("Position %d
/// out of range in doc string file")` is unreachable through this path because
/// `Fdocumentation_property` never reaches the seek for them.
#[test]
fn oracle_a_documentation_position_that_points_at_no_record_answers_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (put 'oracle-doc-position 'variable-documentation 7)
  (list (documentation-property 'oracle-doc-position 'variable-documentation t)
        (progn (put 'oracle-doc-position 'variable-documentation 0)
               (documentation-property 'oracle-doc-position 'variable-documentation t))
        (progn (put 'oracle-doc-position 'variable-documentation -1)
               (documentation-property 'oracle-doc-position 'variable-documentation t))
        (progn (put 'oracle-doc-position 'variable-documentation 12345678901)
               (documentation-property 'oracle-doc-position 'variable-documentation t))))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
