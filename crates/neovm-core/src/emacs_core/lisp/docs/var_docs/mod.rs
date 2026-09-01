//! Documentation strings for built-in (DEFVAR_*) variables.
//!
//! Phase A7-A10 of the substitute-command-keys-audit-v5 R5 plan.
//! Companion to `subr_docs/' (which holds DEFUN docs). Each entry
//! is a `(name, doc)' pair lifted verbatim from a GNU Emacs
//! `DEFVAR_LISP("name", Vsymbol, doc: /* TEXT */)' block (or
//! DEFVAR_INT/BOOL/KBOARD/PER_BUFFER variant) in `src/*.c'.
//!
//! ## Architecture
//!
//! - `gnu_table.rs' is **auto-generated** by
//!   `scripts/extract_gnu_defvar_docs.py' from upstream GNU's
//!   `src/*.c'. To refresh, run the script against an updated GNU
//!   mirror.
//! - `lookup(name)' does a linear scan over the table. Lookups
//!   happen only on `(documentation-property 'foo
//!   'variable-documentation)' queries, which are user-initiated
//!   and rare. Linear scan is fine; ~820 entries today.
//!
//! ## Why grave-quoted strings (not curly)
//!
//! Same reason as `subr_docs/': GNU's `DEFVAR_* doc:' text uses
//! ASCII grave accents (`` ` `` and `'`). `substitute-command-keys'
//! converts them to curly quotes at display time per the user's
//! `text-quoting-style'. Pre-substituting here would lock in
//! `'curve' regardless of preference.
//!
//! ## Lookup precedence: there is ONE source, and this is not it
//!
//! `documentation-property' reads the symbol's plist and nothing
//! else (`src/doc.c:418'). Two things write that plist, in this
//! order:
//!   1. Lisp `defvar'/`defconst'/`defcustom' (`src/eval.c:911', and
//!      only when the doc is non-nil) and `defvaralias'
//!      (`src/eval.c:723'), while the variable is being defined;
//!   2. `Fsnarf_documentation', ONCE, from `lisp/loadup.el:476'
//!      (`:448' here), after the C `DEFVAR's and after every
//!      preloaded Lisp file -- `Fput'ting `etc/DOC's offset over
//!      the top of whatever step 1 left there, for every name that
//!      is bound and whose record does not start with `SKIP'
//!      (`src/doc.c:606-613').
//!
//! **So the snarfed doc is a LAST WRITER, not a fallback**, and
//! ledger 182 is the entry that turned this module around to match:
//! [`lookup`] is step 2's scan and its result can only be written to
//! a plist, while reads go through [`DocImage::text_at`], which is
//! keyed by position. Measured over these 894 names in GNU 31.0.90
//! `-Q --batch': 762 bound, and all 762 carry an INTEGER -- not one
//! carries the string its preloaded Lisp `defvar' put there first.
//!
//! Ledger 178 removed a third source GNU does not have. Two
//! hand-typed tables in `doc.rs', `STARTUP_VARIABLE_DOC_STUBS' and
//! `STARTUP_VARIABLE_DOC_STRING_PROPERTIES', were pre-seeded onto
//! 1972 symbols' plists during bootstrap, which put them ahead of
//! the gate. 35 unbound names answered with a doc where GNU answers
//! nil, and the 70 STUBS names were seeded with the fixnum `0' that
//! `src/doc.c:433-434' reserves for "no doc".

use std::marker::PhantomData;
use std::sync::OnceLock;

pub(crate) mod gnu_table;

// ---------------------------------------------------------------------------
// The `etc/DOC` stand-in, in GNU's own record format
// ---------------------------------------------------------------------------

/// GNU's `etc/DOC`, as this port has it: [`gnu_table::GNU_VAR_DOCS`] laid out
/// in `make-docfile`'s record format, so that the two halves of GNU's design
/// -- the scan that installs offsets and the read that resolves one -- are the
/// same two functions here that they are in `src/doc.c`.
///
/// A record is `^_V<name>\n<text>`, and the number `Fsnarf_documentation`
/// stores on the symbol's plist is the offset of `<text>`:
/// `make_fixnum (pos + end + 1 - buf)` with `end` at the `\n`
/// (`src/doc.c:613`).  The image ends with a final `^_` because
/// `get_doc_string` reads up to the next one (`src/doc.c:220-228`).
///
/// **Why a byte image rather than a row index.**  `Fdocumentation_property`
/// resolves *any* fixnum on the plist, including one Lisp put there, and GNU's
/// answer for a fixnum that does not point at a record is nil -- the check at
/// `src/doc.c:254-260`, which walks backwards over the name to the `^_`.  A
/// row index has no invalid values, so it cannot reproduce that; a byte offset
/// reproduces it exactly, and `(put 'x 'variable-documentation 7)` answers nil
/// in both editors because 7 lands inside the first record's header.
///
/// GNU's `^A` unescaping (`src/doc.c:264-283`) has no counterpart here:
/// `make-docfile` writes `^A` escapes for the three bytes that would be
/// ambiguous in a text file, and [`gnu_table`] is Rust source carrying the
/// unescaped text already.
pub(crate) struct DocImage {
    bytes: &'static [u8],
    /// Byte offset of each `GNU_VAR_DOCS` row's text, parallel to the table.
    positions: Vec<usize>,
}

static DOC_IMAGE: OnceLock<DocImage> = OnceLock::new();

/// The image, built once per process on the first `Snarf-documentation`
/// (`lisp/loadup.el:448`) and read from afterwards.
pub(crate) fn doc_image() -> &'static DocImage {
    DOC_IMAGE.get_or_init(DocImage::build)
}

impl DocImage {
    fn build() -> Self {
        let mut bytes = Vec::with_capacity(512 * 1024);
        let mut positions = Vec::with_capacity(gnu_table::GNU_VAR_DOCS.len());
        for (name, doc) in gnu_table::GNU_VAR_DOCS {
            bytes.push(0x1f);
            bytes.push(b'V');
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(b'\n');
            positions.push(bytes.len());
            bytes.extend_from_slice(doc.as_bytes());
        }
        // `get_doc_string' reads to the next `^_'; the last record needs one.
        bytes.push(0x1f);
        Self {
            bytes: Vec::leak(bytes),
            positions,
        }
    }

    /// GNU's `get_doc_string (POSITION, 0)` for the `etc/DOC` case: check that
    /// POSITION points just past a `^_V<name>\n` header, and return the text
    /// up to the next `^_`.
    ///
    /// `src/doc.c:254-260`:
    ///
    /// ```c
    ///   int test = 1;
    ///   if (get_doc_string_buffer[offset - test++] != '\n')
    ///     return Qnil;
    ///   while (get_doc_string_buffer[offset - test] > ' ')
    ///     test++;
    ///   if (get_doc_string_buffer[offset - test] != '\037')
    ///     return Qnil;
    /// ```
    ///
    /// `None` is where GNU returns nil.  A position outside the image is GNU's
    /// `error ("Position %d out of range in doc string file")`; this port has
    /// no file to be out of range of, so it answers nil there too.
    pub(crate) fn text_at(&'static self, position: i64) -> Option<&'static str> {
        let offset = usize::try_from(position).ok()?;
        if offset == 0 || offset >= self.bytes.len() {
            return None;
        }
        let mut test = 1_usize;
        if *self.bytes.get(offset.checked_sub(test)?)? != b'\n' {
            return None;
        }
        test += 1;
        while *self.bytes.get(offset.checked_sub(test)?)? > b' ' {
            test += 1;
        }
        if *self.bytes.get(offset.checked_sub(test)?)? != 0x1f {
            return None;
        }
        let end = self.bytes[offset..]
            .iter()
            .position(|byte| *byte == 0x1f)
            .map_or(self.bytes.len(), |index| offset + index);
        std::str::from_utf8(&self.bytes[offset..end]).ok()
    }
}

/// GNU's `SKIP` marker is not documentation, and this is where that becomes
/// unrepresentable rather than merely absent.
///
/// A variable that several window-system files declare keeps its text in one
/// of them and a placeholder in the rest -- `x-pointer-shape` is `DEFVAR_LISP`
/// in `src/xfns.c:10327`, `src/w32fns.c:11809`, `src/haikufns.c:3284` and
/// `src/androidfns.c:3587`, three of which read
/// `doc: /* SKIP: real doc in xfns.c.  */` -- so the string is maintained once.
/// 170 `DEFVAR` blocks across GNU's `src/*.c` carry it, and
/// `Fsnarf_documentation` refuses every one:
///
/// ```text
/// /* Ignore docs that start with SKIP.  These mark
///    placeholders where the real doc is elsewhere.  */
/// if ((!NILP (Fboundp (sym)) || !NILP (Fmemq (sym, delayed_init)))
///     && strncmp (end, "\nSKIP", 5))
///   Fput (sym, Qvariable_documentation, make_fixnum (pos + end + 1 - buf));
/// ```
///
/// (`src/doc.c:600-608`.)  So no GNU build shows one to a user.
/// [`gnu_table`] is generated from ALL of `src/*.c` and used to keep the
/// alphabetically first copy of a duplicated name, which handed 35 variables a
/// placeholder instead of their text; the generator now drops a `SKIP` block
/// so the next file's real copy wins.  The check below is what keeps that
/// true: a regenerated table carrying a placeholder does not compile.
const fn doc_is_a_skip_placeholder(doc: &str) -> bool {
    let bytes = doc.as_bytes();
    bytes.len() >= 4 && bytes[0] == b'S' && bytes[1] == b'K' && bytes[2] == b'I' && bytes[3] == b'P'
}

const _: () = {
    let mut index = 0;
    while index < gnu_table::GNU_VAR_DOCS.len() {
        assert!(
            !doc_is_a_skip_placeholder(gnu_table::GNU_VAR_DOCS[index].1),
            "GNU_VAR_DOCS holds a SKIP placeholder; GNU never installs one \
             (src/doc.c:600-608). Re-run scripts/extract_gnu_defvar_docs.py."
        );
        index += 1;
    }
};

/// Look up the doc string for a built-in variable by name.
/// Returns `None` if no entry exists.
///
/// O(n) linear scan over `gnu_table::GNU_VAR_DOCS`. Called only on
/// documentation-query paths, never from `eval`/`funcall`/dispatch.
///
/// The argument is a [`SnarfedVariable`] rather than a `&str` because the
/// table is not a list of answers -- it is a stand-in for `etc/DOC`, which is
/// bigger than any one build, and reading a record out of it is only legal
/// after `Fsnarf_documentation`'s `Fboundp` gate has said yes.
///
/// The *return* is a [`SnarfedDoc`] rather than a `&'static str` for the
/// mirror-image reason, and that is ledger 178's correction: a gate whose
/// answer is an `Option<&'static str>` composes with `or_else`, so a second
/// doc source of the same type can be spliced in *after* the gate has said no
/// and the compiler will not object.  That is exactly what happened -- the
/// pre-176 code read
///
/// ```ignore
/// snarfed.and_then(var_docs::lookup).or_else(|| startup_variable_doc_stub(sym))
/// ```
///
/// and the `or_else` answered for 35 unbound names the gate had refused.
/// `Option<SnarfedDoc>` does not unify with `Option<&'static str>`, so the
/// same line is now a type error rather than a review comment.
///
/// Ledger 182 finished the job by taking the text away from the return value
/// as well.  178's `SnarfedDoc::text()` still handed a reader a `&'static str`
/// keyed by NAME, which is what a *fallback* needs; GNU has no such reader.
/// Its two functions are this one -- the scan `Fsnarf_documentation` runs once
/// over `etc/DOC`, whose result goes onto the plist and nowhere else -- and
/// `get_doc_string`, which is keyed by POSITION and is
/// [`DocImage::text_at`].  So [`SnarfedDoc`] now yields a position and is
/// consumed doing it.
#[inline]
pub(crate) fn lookup(variable: SnarfedVariable<'_>) -> Option<SnarfedDoc<'_>> {
    let index = gnu_table::GNU_VAR_DOCS
        .iter()
        .position(|(n, _)| *n == variable.name)?;
    Some(SnarfedDoc {
        position: doc_image().positions[index],
        gate: PhantomData,
    })
}

/// A record [`lookup`] found in the `etc/DOC` stand-in, carrying the proof
/// that `Fsnarf_documentation`'s gate was passed to get it.
///
/// The lifetime borrows the [`SnarfedVariable`] the gate produced, so the
/// proof cannot outlive the question.  There is deliberately no constructor
/// from a bare `&str`, no `Default`, and -- since ledger 182 -- **no way to
/// get the text out**: the only thing that can be done with one is
/// [`position`](Self::position), the number GNU's `Fput` stores.  A doc source
/// that can only be written to a plist cannot be spliced in as a fallback
/// *after* the plist has been consulted, which is the precedence GNU has
/// (`lisp/loadup.el:251` writes, `:476` overwrites) and the one this port had
/// backwards.
#[derive(Debug)]
pub(crate) struct SnarfedDoc<'a> {
    position: usize,
    gate: PhantomData<SnarfedVariable<'a>>,
}

impl SnarfedDoc<'_> {
    /// The value GNU stores on the plist: `make_fixnum (pos + end + 1 - buf)`,
    /// the offset of the record's text (`src/doc.c:613`).
    ///
    /// Consumes the proof, because the gate is asked once per record per snarf
    /// and the answer belongs on the symbol rather than in a caller's hand.
    #[inline]
    pub(crate) fn position(self) -> i64 {
        self.position as i64
    }
}

/// A built-in variable name that **this build binds** -- the only kind
/// `Fsnarf_documentation` installs a doc string for, and therefore the only
/// key [`lookup`] accepts.
///
/// GNU's DOC file is written by `make-docfile`, a text scanner that does not
/// evaluate the preprocessor and does not know which files this build
/// compiles, so `etc/DOC` names variables no build has.  GNU filters at the
/// other end instead, once, at dump time (`lisp/loadup.el:476`):
///
/// ```c
/// /* Ignore docs that start with SKIP.  These mark
///    placeholders where the real doc is elsewhere.  */
/// if ((!NILP (Fboundp (sym)) || !NILP (Fmemq (sym, delayed_init)))
///     && strncmp (end, "\nSKIP", 5))
///   Fput (sym, Qvariable_documentation, make_fixnum (pos + end + 1 - buf));
/// ```
///
/// (`src/doc.c:606-613`; the comment fifteen lines up, `src/doc.c:585-594`,
/// says GNU used to filter by `build_files` and now relies on this.)  The
/// `Fput` is the whole branch: **an unbound name's doc is not recorded
/// differently, it is not recorded at all**, so `documentation-property`
/// answers nil.  Measured over the 881 names entry 168 left in this table,
/// GNU 31.0.90 `-Q --batch`: 751 bound names have a doc, 130 unbound names
/// have nil, and there is no name on either diagonal.
///
/// Only boundness can decide this, which is why entry 168's `SKIP` prefix test
/// was not enough and why a hand-written list of "names GNU leaves unbound
/// here" is the wrong instrument.  Three examples the text cannot see:
/// `internal-interpreter-environment` is `DEFVAR_LISP`'d at `src/eval.c:4569`
/// and **uninterned three lines later** (`src/eval.c:4578`, "Don't export this
/// variable to Elisp"); `x-mode-pointer-shape`'s only declarations sit inside
/// `#if false` (`src/xfns.c:10333-10352`) while `echo-area-clear-hook`'s sits
/// inside `#if 0` but is bound by an `Fset` on the line after the `#endif`
/// (`src/keyboard.c:14058-14076`), so the two `#if 0` cases split; and
/// `xft-font-ascent-descent-override` is real, documented and compiled out
/// whenever Cairo beats Xft (`configure.ac:7228-7231`).
///
/// The `Fmemq (sym, delayed_init)` half of GNU's condition is a Lisp-level
/// escape hatch for preloaded `custom-initialize-delay` defcustoms, which
/// `lisp/custom.el:142-161` marks special and deliberately leaves unbound.
/// It is not reachable from this table: no C `DEFVAR` name is on
/// `custom-delayed-init-variables`, which is exactly why the 751/130 split
/// above has no exceptions.
pub(crate) struct SnarfedVariable<'a> {
    name: &'a str,
}

impl<'a> SnarfedVariable<'a> {
    /// GNU's `Fboundp (sym)`, asked of this build's obarray.
    ///
    /// `None` -- the symbol is unbound here -- is the answer that means "this
    /// build has no such variable, so it has no documentation either".
    ///
    /// The question is asked of the **global/default** binding, not the
    /// current buffer's: GNU asks it once during `loadup`, in a `*scratch*`
    /// with no buffer-local bindings, and the result is a property on the
    /// symbol rather than something re-decided per query.  Constants
    /// (`most-positive-fixnum` and friends) count as bound for the same reason
    /// `Fboundp` says so -- `SYMBOL_CONSTANT_P` is a write barrier, not an
    /// unbound value cell.
    #[inline]
    pub(crate) fn if_bound_in(
        obarray: &crate::emacs_core::symbol::Obarray,
        id: crate::emacs_core::intern::SymId,
        name: &'a str,
    ) -> Option<Self> {
        (obarray.boundp_id(id) || obarray.is_constant_id(id)).then_some(Self { name })
    }
}
