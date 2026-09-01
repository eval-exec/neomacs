//! Documentation strings for built-in (subr) functions.
//!
//! Phase A of the substitute-command-keys-audit-v5 R5 plan
//! (Option A inline storage). Each entry is a `(name, doc)` pair
//! lifted verbatim from a GNU Emacs `DEFUN ("name", ..., doc: /*
//! TEXT */)` block in `src/*.c`. Strings live in `.rodata` and are
//! looked up by name from `function_doc_or_error` and
//! `builtin_internal_subr_documentation`.
//!
//! ## Architecture
//!
//! - `gnu_table.rs` is **auto-generated** by
//!   `scripts/extract_gnu_defun_docs.py`, whose scanner
//!   (`scripts/make_docfile.py`) is a port of GNU's own
//!   `lib-src/make-docfile.c` and is verified byte-identical against the
//!   compiled binary whenever the GNU mirror has one. To refresh, run the
//!   script against an updated GNU mirror — the diff is mechanical and
//!   reviewable.
//! - `lookup` does a linear scan over the table. The table is
//!   ~1,700 entries today, lookups happen rarely (only on
//!   `(documentation 'foo)` queries), and the doc-query path is not
//!   on any hot loop. Linear scan is fine; if it ever shows up in a
//!   profile, swap to `phf::Map` or sorted-array binary search
//!   without changing the call-site signature.
//!
//! ## Why the argument is a [`SnarfedSubr`] and not a `&str`
//!
//! `Fsnarf_documentation` does not install a function's doc string
//! unconditionally.  Its function arm is one `if`:
//!
//! ```c
//! /* Attach a docstring to a function?  */
//! else if (p[1] == 'F')
//!   {
//!     if (!NILP (Ffboundp (sym)) && strncmp (end, "\nSKIP", 5))
//!       store_function_docstring (sym, pos + end + 1 - buf);
//!   }
//! ```
//!
//! (`src/doc.c:617-621`.)  Two clauses: the name must be **fbound**, and the
//! text must not be GNU's `SKIP` placeholder.  `etc/DOC` is deliberately
//! bigger than any one build -- `make-docfile` is a text scanner that does not
//! evaluate the preprocessor and is handed `$(SOME_MACHINE_OBJECTS)` on top of
//! the build's own objects (`src/Makefile.in:657-667`) -- so both clauses
//! exist to undo, at load time, the fact that the file over-collects.
//! `gnu_table` stands in for `etc/DOC` and over-collects the same way, so both
//! clauses have to be somewhere.
//!
//! The `SKIP` clause is in the generator: a placeholder is never written into
//! the table at all, and the `const` block below refuses to compile one.
//!
//! The `Ffboundp` clause is in the **type**.  Ledger 173 had to build
//! `var_docs::SnarfedVariable` to carry the same question for variables,
//! because `documentation-property` is asked about a *name* and a name proves
//! nothing.  A function's documentation is asked about a *function value*:
//! `function_doc_or_error` reaches this table only from
//! `ValueKind::Subr(id)` / `ValueKind::Veclike(VecLikeType::Subr)`, and
//! `internal-subr-documentation` only after `as_subr_id` said yes.  A `Value`
//! that is a subr is already a proof that `Ffboundp` would answer t, and a
//! stronger one -- GNU's `store_function_docstring` goes on to require
//! `SUBRP (fun)` before it writes the offset (`src/doc.c:471-475`), so a name
//! that is fbound to something else gets no C doc either.
//!
//! [`SnarfedSubr`] is that proof made into a type: its only constructor takes
//! a `Value` and hands back `None` unless the value is a subr, so there is no
//! way to reach the table from a bare `&str` and no way for a later `or_else`
//! to splice a second answer in behind the gate -- ledger 178's correction to
//! 173, applied here before it can happen rather than after.
//!
//! ## Why no `SubrObj.doc` field
//!
//! Storing the doc as a `&'static str` field on `SubrObj` would
//! save ~10 ns per query (one cache-line load instead of a linear
//! scan), but reading the field from a `Value` requires
//! `unsafe { &*(ptr as *const SubrObj) }`. The user explicitly
//! asked to avoid `unsafe`. The 10 ns difference is invisible for
//! doc queries (which run at ~10/sec, not 10⁹/sec), so the central
//! table is the better trade.
//!
//! ## Why grave-quoted strings (not curly)
//!
//! GNU's `DEFUN doc:` text uses ASCII grave accents (`` ` `` and
//! `'`) for quotes. `substitute-command-keys` (in `lisp/help.el`)
//! converts them to ‘ ’ at display time per the user's
//! `text-quoting-style`. Pre-substituting here would lock in
//! `'curve` regardless of preference (audit v5 §2.4).

use crate::emacs_core::intern::resolve_sym;
use crate::emacs_core::value::{Value, ValueKind, VecLikeType};

pub(crate) mod gnu_table;

/// The name of a function that is a **subr in this build**, which is the
/// function half of `Fsnarf_documentation`'s gate.
///
/// Constructing one is the only way to read `gnu_table`, so the question
/// "would GNU have installed this doc?" is asked before the table is, by
/// construction rather than by convention.
pub(crate) struct SnarfedSubr {
    name: &'static str,
}

impl SnarfedSubr {
    /// `Ffboundp (sym) && SUBRP (XSYMBOL (sym)->u.s.function)`, as a type.
    ///
    /// Both of GNU's subr representations count: `ValueKind::Subr` is the
    /// immediate form and `Veclike(VecLikeType::Subr)` the heap-allocated
    /// one, and `Fsnarf_documentation` cannot tell them apart because GNU
    /// has only the second.
    pub(crate) fn of(function: Value) -> Option<Self> {
        let id = match function.kind() {
            ValueKind::Subr(id) => id,
            ValueKind::Veclike(VecLikeType::Subr) => function.as_subr_id()?,
            _ => return None,
        };
        Some(Self {
            name: resolve_sym(id),
        })
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }
}

/// GNU's own test, verbatim: a DOC entry that starts with `SKIP` is not
/// documentation.
///
/// A function several window-system files define carries the real text in
/// exactly one of them and `doc: /* SKIP: real doc in xfns.c.  */` in the
/// rest, so the string is maintained once (`src/doc.c:585-594`).
/// `Fsnarf_documentation` refuses to install such a record --
/// `strncmp (end, "\nSKIP", 5)` guarding `store_function_docstring`
/// (`src/doc.c:617-621`) -- so no GNU build ever shows one to a user, and a
/// table row holding one is a generator bug rather than a value.
const fn doc_is_a_skip_placeholder(doc: &str) -> bool {
    let bytes = doc.as_bytes();
    bytes.len() >= 4 && bytes[0] == b'S' && bytes[1] == b'K' && bytes[2] == b'I' && bytes[3] == b'P'
}

/// `read_c_string_or_comment` discards **all** whitespace at the head of a
/// `doc:` comment, newlines included:
///
/// ```c
/// c = getc (infile);
/// if (comment)
///   while (c_isspace (c))
///     c = getc (infile);
/// ```
///
/// (`lib-src/make-docfile.c:416-419`.)  So no record GNU writes can begin
/// with a space or a tab, whatever the C source's indentation.
const fn doc_starts_with_a_space_or_tab(doc: &str) -> bool {
    let bytes = doc.as_bytes();
    !bytes.is_empty() && (bytes[0] == b' ' || bytes[0] == b'\t')
}

/// A record may begin with a newline in exactly one situation: the comment
/// body was **empty** (`doc: /* */`, as at `src/xfaces.c:7335`, `7368` and
/// `4033`), and everything present is the `"\n\n(fn ARGS)"` that
/// `write_c_args` appends afterwards (`lib-src/make-docfile.c:1215-1218`).
/// Any other leading newline means the body's own leading whitespace survived,
/// which is the bug ledger 173 found on the variable side and ledger 181 on
/// this one.
const fn doc_starts_with_a_newline_but_has_a_body(doc: &str) -> bool {
    let bytes = doc.as_bytes();
    if bytes.is_empty() || bytes[0] != b'\n' {
        return false;
    }
    // `"\n\n(fn"` is the only legal prefix, and nothing may precede it.
    !(bytes.len() >= 5
        && bytes[1] == b'\n'
        && bytes[2] == b'('
        && bytes[3] == b'f'
        && bytes[4] == b'n')
}

/// `put_char` holds trailing spaces and newlines in
/// `pending_spaces`/`pending_newlines` and emits them only when a non-space
/// character follows (`lib-src/make-docfile.c:279-310`), so whatever sits
/// between the last real character and `*/` never reaches the DOC file.
const fn doc_ends_with_whitespace(doc: &str) -> bool {
    let bytes = doc.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let last = bytes[bytes.len() - 1];
    last == b' ' || last == b'\t' || last == b'\n' || last == b'\r'
}

/// Four properties every record `make-docfile` writes has, checked at compile
/// time over every row.
///
/// These are predicates over rows that exist, and ledger 173's law says such a
/// check cannot see a row that was never written -- ask what it reports when
/// the table is EMPTY and the answer is "green".  So it is deliberately only
/// half the guard: the other half lives in the generator, which diffs its
/// whole DOC stream against GNU's compiled `make-docfile` byte for byte and
/// refuses to write the table on any difference, in either direction.  What
/// this block adds is that no hand edit can reintroduce the failures after
/// generation, and that the reason each row shape is illegal is written down
/// next to the row shape.
const _: () = {
    let mut index = 0;
    while index < gnu_table::GNU_SUBR_DOCS.len() {
        let doc = gnu_table::GNU_SUBR_DOCS[index].1;
        assert!(
            !doc_is_a_skip_placeholder(doc),
            "GNU_SUBR_DOCS holds a SKIP placeholder; Fsnarf_documentation never \
             installs one (src/doc.c:617-621). Re-run \
             scripts/extract_gnu_defun_docs.py."
        );
        assert!(
            !doc_starts_with_a_space_or_tab(doc),
            "GNU_SUBR_DOCS holds a doc starting with a space or tab; \
             read_c_string_or_comment strips all leading whitespace \
             (lib-src/make-docfile.c:416-419). Re-run \
             scripts/extract_gnu_defun_docs.py."
        );
        assert!(
            !doc_starts_with_a_newline_but_has_a_body(doc),
            "GNU_SUBR_DOCS holds a doc starting with a newline before real \
             text; only an EMPTY doc: comment may begin with the \"\\n\\n(fn\" \
             that write_c_args appends. Re-run \
             scripts/extract_gnu_defun_docs.py."
        );
        assert!(
            !doc_ends_with_whitespace(doc),
            "GNU_SUBR_DOCS holds a doc ending in whitespace; put_char never \
             flushes pending spaces or newlines (lib-src/make-docfile.c:279-310). \
             Re-run scripts/extract_gnu_defun_docs.py."
        );
        index += 1;
    }
};

/// Look up the doc string for a subr. Returns `None` if the table has no
/// entry, which is GNU's answer too: a subr whose `DEFUN` this port defines
/// and GNU does not has no `etc/DOC` record either.
///
/// O(n) linear scan over `gnu_table::GNU_SUBR_DOCS`. Called only on
/// documentation-query paths, never from `eval`/`funcall`/dispatch.
#[inline]
pub(crate) fn lookup(subr: &SnarfedSubr) -> Option<&'static str> {
    gnu_table::GNU_SUBR_DOCS
        .iter()
        .find(|(n, _)| *n == subr.name())
        .map(|(_, doc)| *doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GNU's `src/*.c` holds 1733 `DEFUN` doc records once
    /// `Fsnarf_documentation`'s `SKIP` clause has refused the placeholders.
    ///
    /// A floor rather than an equality, because the mirror moves; a floor
    /// rather than a `is_empty()` check, because the failure this guards is
    /// silent shrinkage.  The number is what the generator printed against
    /// GNU 31.0.90 (`0ee48ac4df2`).  Ledger 181's whole finding was 30 rows
    /// missing from a table nobody had counted, so the count is pinned.
    #[test]
    fn gnu_subr_docs_is_not_quietly_smaller_than_gnus_source() {
        assert!(
            gnu_table::GNU_SUBR_DOCS.len() >= 1733,
            "GNU_SUBR_DOCS has {} rows; GNU's src/*.c has 1733 non-SKIP DEFUN \
             doc records. A generator that drops rows is silent from every \
             direction except this one.",
            gnu_table::GNU_SUBR_DOCS.len()
        );
    }

    /// The table is sorted and free of duplicate names, so a linear scan and a
    /// future binary search cannot disagree about which row a name resolves to.
    #[test]
    fn gnu_subr_docs_is_sorted_and_has_no_duplicate_names() {
        let mut previous: Option<&str> = None;
        for (name, _) in gnu_table::GNU_SUBR_DOCS {
            if let Some(prev) = previous {
                assert!(
                    prev < *name,
                    "GNU_SUBR_DOCS is not strictly sorted: {prev:?} then {name:?}"
                );
            }
            previous = Some(name);
        }
    }

    /// The six names ledger 181 found missing or wrong, one per mechanism, so
    /// a regression names its own cause.
    ///
    /// * `native-comp-function-p` -- the head regex needed a line break after
    ///   the interactive spec and `data.c:1067` has none.
    /// * `define-charset-internal` -- MIN is the C identifier
    ///   `charset_arg_max` (`charset.c:845`), not a literal.
    /// * `window-parameter` -- `window.c:2390` spells the marker `doc:  /*`
    ///   with two spaces, so the unbounded literal search took
    ///   `set-window-parameter`'s comment instead.
    /// * `set-window-parameter` -- and was then skipped as collateral.
    /// * `x-display-list` -- `androidfns.c` supplies the alphabetically first
    ///   copy, which is GNU's `SKIP` placeholder.
    /// * `string` -- `character.c`-style `doc: /*` followed by a newline left
    ///   a leading newline in the stored text.
    #[test]
    fn the_six_mechanisms_ledger_181_found_are_each_represented() {
        let rows: std::collections::HashMap<&str, &str> =
            gnu_table::GNU_SUBR_DOCS.iter().copied().collect();
        for (name, first_line) in [
            (
                "native-comp-function-p",
                "Return t if the object is native-compiled Lisp function, nil otherwise.",
            ),
            ("define-charset-internal", "For internal use only."),
            ("window-parameter", "Return WINDOW's value for PARAMETER."),
            (
                "set-window-parameter",
                "Set WINDOW's value of PARAMETER to VALUE.",
            ),
            (
                "x-display-list",
                "Return the list of display names that Emacs has connections to.",
            ),
            (
                "string",
                "Concatenate all the argument characters and make the result a string.",
            ),
        ] {
            let doc = rows
                .get(name)
                .unwrap_or_else(|| panic!("GNU_SUBR_DOCS has no row for {name:?}"));
            assert_eq!(
                doc.lines().next().unwrap_or(""),
                first_line,
                "GNU_SUBR_DOCS row for {name:?} is not GNU's text"
            );
        }
    }
}
