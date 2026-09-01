//! GNU's `DEFVAR_LISP' and `DEFVAR_KBOARD' declarations, in one table.
//!
//! `DEFVAR_BOOL' and `DEFVAR_INT' are declared here variable by variable
//! (`defvar_bool.rs', `Obarray::define_int_variable') because the C slot's
//! TYPE is the thing being ported: a `bool *' coerces and an `intmax_t *'
//! type-checks, and a registration site that forgot would silently accept
//! values GNU refuses (ledger 132).  `DEFVAR_LISP' has no such rule -- its
//! store arm is a plain assignment (`src/data.c:1489-1516') -- which is why
//! ledger 132 concluded "no divergence follows" and why entries 135, 138 and
//! 141 carried that sentence forward.
//!
//! It is wrong, and 447 names are the measure of how wrong (ledger 168,
//! re-derived by ledger 170).  Being `SYMBOL_FORWARDED' costs three things
//! that have nothing to do with the store arm, because GNU's redirect switch
//! reaches them BEFORE it ever looks at what the descriptor points to:
//!
//! 1. `set_internal' refuses an unbind -- `error ("Built-in variable may not
//!    be unbound : %s")' (`src/data.c:1802-1809'), and the
//!    localized-with-forwarder twin at `src/data.c:1723-1727'.
//! 2. `Fdefvaralias' refuses the symbol as a NEW-ALIAS -- `error ("Cannot
//!    make a built-in variable an alias: %s")' (`src/eval.c:665-668').
//! 3. For `Lisp_Fwd_Kboard_Obj' only, `Fmake_variable_buffer_local' and
//!    `Fmake_local_variable' refuse it -- `error ("Symbol %s may not be
//!    buffer-local")' (`src/data.c:2220-2223', `src/data.c:2287-2288') -- and
//!    `variable-binding-locus' answers the terminal rather than nil
//!    (`src/data.c:2523-2525').
//!
//! Every one of those keys on the symbol's REDIRECT TAG, so the tag is the
//! whole of what has to be ported; the C global it points at does not have to
//! exist.  It does exist here anyway, in the leaked descriptor, for the same
//! reason [`LispIntFwd`](crate::emacs_core::forward::LispIntFwd)'s does: a
//! symbol's value cell is one word, and once it holds the descriptor pointer
//! the value needs somewhere else to live.  That is not the failure class
//! entries 161-163 closed -- a leaked descriptor owning a heap `Value' is
//! already how `DEFVAR_INT' works here, and `Obarray::trace_roots' has rooted
//! those descriptors since, exactly as `staticpro' roots GNU's
//! `struct emacs_globals' slots and `mark_kboards' roots its `KBOARD' ones.
//!
//! # Why a table, and why it is only a declaration
//!
//! GNU's declarations are spread over 60-odd `syms_of_*' functions and this
//! port's are spread over as many `register_bootstrap_vars' functions plus
//! several hundred bare `set_symbol_value' calls, so there is no single site
//! that could carry the fact.  [`gnu_table`] is generated from GNU's `src/*.c'
//! by `scripts/extract_gnu_defvar_object_names.py' and lists every name GNU
//! declares, with no measurement of this port in it: [`adopt`] simply finds no
//! symbol to adopt for a name this build does not have, and reports how many.
//! That keeps the table refreshable from an updated GNU mirror and stops it
//! from going stale as the port grows a variable.

pub(crate) mod gnu_table;

use super::symbol::{Obarray, SymbolRedirect};
use super::value::Value;

/// Which `Lisp_Fwd' variant a GNU declaration gives the symbol.
///
/// A closed set of two, because those are the two macros that produce a
/// Lisp-object forward: `DEFVAR_LISP'/`DEFVAR_LISP_NOPRO' produce
/// `Lisp_Fwd_Obj' and `DEFVAR_KBOARD' produces `Lisp_Fwd_Kboard_Obj'
/// (`src/lisp.h:3494-3524').  The distinction is Lisp-visible -- only the
/// keyboard variant refuses `make-local-variable' -- so it is a variant here
/// rather than a flag a row could omit.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GnuObjectForward {
    /// `DEFVAR_LISP` / `DEFVAR_LISP_NOPRO` -> GNU `Lisp_Fwd_Obj`.
    Global,
    /// `DEFVAR_KBOARD` -> GNU `Lisp_Fwd_Kboard_Obj`.
    Keyboard,
}

impl GnuObjectForward {
    /// The same variant, spelled the way a BLV records the forwarder it is
    /// carrying.  One conversion in one place beats two parallel matches.
    fn localized_dump_kind(self) -> crate::emacs_core::pdump::types::DumpLocalizedForwarder {
        use crate::emacs_core::pdump::types::DumpLocalizedForwarder as Kind;
        match self {
            Self::Global => Kind::Obj,
            Self::Keyboard => Kind::Kboard,
        }
    }
}

/// Whether GNU KEEPS the `declared_special` flag its `DEFVAR_*` just set.
///
/// `defvar_lisp_nopro` sets it unconditionally (`src/lread.c:5274`), so the
/// overwhelming majority of rows are [`Special`](Self::Special) and the flag
/// could be read straight off the `DEFVAR` head.  Three names in GNU's `src/`
/// have it taken back off again on a following line, and because that line
/// FOLLOWS the declaration, a generator that scrapes `DEFVAR` heads is
/// structurally unable to see it -- which is exactly how ledger 176's
/// `features` divergence got in, and the same shape as ledger 173's extractor
/// bugs one file over.
///
/// It is a field on the declaration row rather than a separate list because
/// the failure mode being designed out is DRIFT between the two halves: a
/// hand-maintained exception list beside a generated declaration table is one
/// GNU spelling away from being silently wrong, and was.  Making it a variant
/// means the generator has to decide for every row, an exception cannot exist
/// without the declaration it belongs to, and adding a row without stating
/// which it is does not compile.
///
/// GNU spells the un-declaration two ways, and both count:
///
/// * `Fmake_var_non_special (Qfeatures);` -- a call to the same primitive Lisp
///   reaches as `internal-make-var-non-special` (`src/fns.c:6823`, declared at
///   `src/fns.c:6817`), under the comment "Let people use lexically scoped
///   vars named `features'."
/// * a direct store the line names the symbol in --
///   `XSYMBOL (Qtop_level)->u.s.declared_special = false;`
///   (`src/keyboard.c:13955`, declared at `:13951`) and
///   `XBARE_SYMBOL (intern ("values"))->u.s.declared_special = false;`
///   (`src/lread.c:5596`, declared at `:5592`).
///
/// The other two `declared_special = false` sites in GNU are NOT exceptions of
/// this shape, and what disqualifies them is structural rather than which file
/// they are in: neither names a symbol.  `src/alloc.c:3672` is
/// `p->u.s.declared_special = false` initialising a freshly allocated symbol,
/// and `src/eval.c:1071` is the body of `internal-make-var-non-special`
/// itself, clearing the flag on whatever symbol it was handed at runtime.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GnuSpecialness {
    /// `DEFVAR_*` alone: the symbol stays special, so `let` under
    /// `lexical-binding` rebinds the global dynamically.
    Special,
    /// `DEFVAR_*` and then an explicit un-declaration: `let` under
    /// `lexical-binding` makes an ordinary lexical variable the global never
    /// sees.
    NonSpecial,
}

/// One GNU `DEFVAR_LISP' / `DEFVAR_KBOARD' declaration.
#[derive(Copy, Clone, Debug)]
pub struct GnuObjectVariable {
    /// The Lisp symbol name the macro's first argument spells.
    pub name: &'static str,
    /// The forward variant the macro installs.
    pub kind: GnuObjectForward,
    /// Whether GNU keeps the `declared_special` flag the macro sets.
    pub special: GnuSpecialness,
}

/// What [`adopt_one`] did with a row.
///
/// Every arm is a state this port can genuinely be in, and naming them is what
/// lets [`adopt`] report counts a ledger entry can be checked against instead
/// of a bare "done".
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Adoption {
    /// The symbol became `SYMBOL_FORWARDED`; its value moved into the
    /// descriptor.
    Forwarded,
    /// The symbol is not interned in this obarray: this build has no such
    /// variable.  GNU has 49 of these relative to this port (ledger 170).
    Absent,
    /// Interned but unbound.  GNU's `DEFVAR_LISP` always binds, so this is a
    /// variable this port does not have either -- the name is merely reachable
    /// because some other code mentions it.
    Unbound,
    /// Already `SYMBOL_FORWARDED` -- a `DEFVAR_INT`/`DEFVAR_BOOL` name that
    /// GNU also spells `DEFVAR_LISP` in a platform file, or a re-run.
    AlreadyForwarded,
    /// `SYMBOL_VARALIAS`: this port made the name an alias of something else.
    /// GNU's C would have forwarded it first and `Fdefvaralias` would then
    /// have refused, so this arm is a divergence of its own, counted here
    /// rather than papered over.
    Alias,
    /// `SYMBOL_LOCALIZED`: the symbol already carries a BLV, so GNU's
    /// `make_blv` put the forwarder inside it (`src/data.c:2112-2140`) rather
    /// than on the symbol.  GNU reaches this state for every `DEFVAR_LISP`
    /// that its own C then hands to `Fmake_variable_buffer_local` --
    /// `case-fold-search` is declared and localized four lines apart
    /// (`src/buffer.c:5981-5985`) -- and `set_internal` refuses the unbind
    /// from `if (unbinding_p && blv->fwd)` instead
    /// (`src/data.c:1723-1727`).
    Localized,
}

/// Counts of every [`Adoption`] outcome over the whole table.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct AdoptionCounts {
    pub forwarded: usize,
    pub absent: usize,
    pub unbound: usize,
    pub already_forwarded: usize,
    pub alias: usize,
    pub localized: usize,
}

impl AdoptionCounts {
    fn record(&mut self, outcome: Adoption) {
        match outcome {
            Adoption::Forwarded => self.forwarded += 1,
            Adoption::Absent => self.absent += 1,
            Adoption::Unbound => self.unbound += 1,
            Adoption::AlreadyForwarded => self.already_forwarded += 1,
            Adoption::Alias => self.alias += 1,
            Adoption::Localized => self.localized += 1,
        }
    }

    /// Rows this pass left as plain obarray cells, i.e. the residual on which
    /// `makunbound` still answers differently from GNU for a variable this
    /// port actually has.
    pub fn unforwarded_but_present(self) -> usize {
        self.alias + self.localized
    }
}

/// Give every GNU-declared name this port has GNU's redirect tag.
///
/// Runs at the end of the C-level bootstrap, where GNU's `main` has finished
/// every `syms_of_*` and before any Lisp runs -- the same boundary, reached
/// from the other side: GNU declares first and assigns after, while this port
/// assigns from several hundred scattered sites and declares once here.  The
/// value each symbol already holds moves into the descriptor, so no row can
/// change a variable's value, and a name this build does not have is left
/// alone rather than newly bound to nil.
pub fn adopt(obarray: &mut Obarray) -> AdoptionCounts {
    let mut counts = AdoptionCounts::default();
    for var in gnu_table::GNU_OBJECT_VARIABLES {
        counts.record(adopt_one(obarray, var));
    }
    // GNU's own order: declare every name, then take `declared_special` back
    // off the ones that ask for it (`src/fns.c:6817` then `:6823`).  Driven by
    // the same generated table as the declarations, so the two halves are
    // refreshed from the mirror together and cannot drift -- see
    // [`GnuSpecialness`].
    //
    // Presence is tested exactly as [`adopt_one`] tests it, and for the same
    // reason: a name this build does not have is left alone.  Clearing the flag
    // through the name rather than the id would `intern` it and give the
    // obarray a member for a variable that does not exist here.
    for var in gnu_table::GNU_OBJECT_VARIABLES {
        if var.special != GnuSpecialness::NonSpecial {
            continue;
        }
        let Some(id) = super::intern::lookup_interned(var.name) else {
            continue;
        };
        if obarray.get_by_id(id).is_none() {
            continue;
        }
        obarray.make_non_special_id(id);
    }
    counts
}

fn adopt_one(obarray: &mut Obarray, var: &GnuObjectVariable) -> Adoption {
    let Some(id) = super::intern::lookup_interned(var.name) else {
        return Adoption::Absent;
    };
    let Some(symbol) = obarray.get_by_id(id) else {
        return Adoption::Absent;
    };
    match symbol.redirect() {
        SymbolRedirect::Forwarded => return Adoption::AlreadyForwarded,
        SymbolRedirect::Varalias => return Adoption::Alias,
        SymbolRedirect::Localized => {
            // GNU's `make_blv` moved the descriptor into the BLV, so put an
            // equivalent one there.  Idempotent: `reattach_localized_forwarder`
            // returns immediately when `blv->fwd` is already set, which is how
            // a `DEFVAR_BOOL`/`DEFVAR_INT` variable this port localized keeps
            // the descriptor its own declaration gave it.
            obarray.reattach_localized_forwarder(id, var.kind.localized_dump_kind());
            return Adoption::Localized;
        }
        SymbolRedirect::Plainval => {}
    }
    let Some(value) = obarray.find_symbol_value(id) else {
        return Adoption::Unbound;
    };
    if value == Value::UNBOUND {
        return Adoption::Unbound;
    }
    match var.kind {
        GnuObjectForward::Global => {
            let fwd = crate::emacs_core::forward::alloc_objfwd(value);
            obarray.install_objfwd(id, fwd);
        }
        GnuObjectForward::Keyboard => {
            let fwd = crate::emacs_core::forward::alloc_kboard_objfwd(value);
            obarray.install_kboard_objfwd(id, fwd);
        }
    }
    Adoption::Forwarded
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Measured against GNU Emacs 31.0.90's `src/*.c`, 2026-08-21:
    /// 562 `DEFVAR_LISP`/`DEFVAR_LISP_NOPRO` names and 14 `DEFVAR_KBOARD`
    /// ones.  The generator keeps the first declaration of a name it sees, so
    /// a variable several window-system files declare counts once.
    ///
    /// It was 564 + 14 until ledger 183: the extractor scanned raw C text and
    /// therefore saw seven `DEFVAR` heads parked inside `#if 0`, of which five
    /// (`x-pointer-shape` and four cursor names in `w32fns.c`) are also
    /// declared in `xfns.c` and belong here anyway.  Two are not declared
    /// anywhere else and had no business being rows -- `echo-area-clear-hook`
    /// (`src/keyboard.c:14059`) and `w32-generate-fake-inodes`
    /// (`src/w32proc.c:4831`) -- and the first of them cost two Lisp-visible
    /// facts, because this port DOES have the variable.
    #[test]
    fn table_matches_gnu_counts() {
        let table = gnu_table::GNU_OBJECT_VARIABLES;
        assert_eq!(table.len(), 576);
        assert_eq!(
            table
                .iter()
                .filter(|v| v.kind == GnuObjectForward::Keyboard)
                .count(),
            14
        );
    }

    #[test]
    fn table_has_no_duplicate_rows() {
        let mut names: Vec<&str> = gnu_table::GNU_OBJECT_VARIABLES
            .iter()
            .map(|v| v.name)
            .collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate DEFVAR_LISP/KBOARD row");
    }

    /// `DEFVAR_BOOL` and `DEFVAR_INT` names must NOT be in this table: those
    /// two macros install their own forward types, whose store rules an
    /// `Lisp_Fwd_Obj` would silently drop.
    #[test]
    fn table_and_the_defvar_bool_table_are_disjoint() {
        let bools: std::collections::HashSet<&str> =
            crate::emacs_core::defvar_bool::GNU_BOOL_VARIABLES
                .iter()
                .map(|v| v.name)
                .collect();
        let overlap: Vec<&str> = gnu_table::GNU_OBJECT_VARIABLES
            .iter()
            .map(|v| v.name)
            .filter(|n| bools.contains(n))
            .collect();
        assert_eq!(overlap, Vec::<&str>::new());
    }
}

#[cfg(test)]
mod adoption_tests {
    use super::*;

    /// Re-running the pass on a live `Context` must find every row already
    /// settled.
    ///
    /// This is the guard that keeps the pass at the right point in the
    /// bootstrap: it first ran with the `register_bootstrap_vars` calls, where
    /// six names -- `main-thread`, `system-name`, `user-login-name`,
    /// `user-full-name`, `user-real-login-name`, `operating-system-release` --
    /// are not bound yet, and a re-run forwarding anything is exactly that
    /// symptom.  `alias` is asserted zero because GNU's C would have forwarded
    /// the name before any `defvaralias` could reach it, so a row landing
    /// there is a divergence and not a state to accept.  The other three arms
    /// are counts that move as the port grows a variable, so they are
    /// reported rather than pinned.
    #[test]
    fn adoption_is_idempotent_and_leaves_no_alias_rows() {
        crate::test_utils::init_test_tracing();
        let mut eval = crate::emacs_core::eval::Context::new();
        let counts = adopt(eval.obarray_mut());
        assert_eq!(
            counts.forwarded, 0,
            "the bootstrap pass ran too early and missed rows: {counts:?}"
        );
        assert_eq!(
            counts.alias, 0,
            "a GNU built-in variable is an alias here: {counts:?}"
        );
        assert_eq!(
            counts.already_forwarded + counts.localized,
            counts.already_forwarded + counts.unforwarded_but_present(),
            "{counts:?}"
        );
    }

    /// The `Localized` rows carry the forwarder in the BLV, GNU's `make_blv`
    /// (`src/data.c:2112-2140`), so `set_internal` still refuses their unbind
    /// through `blv->fwd` (`src/data.c:1723-1727`).  Measured under GNU Emacs
    /// 31.0.90 `-Q --batch`, `case-fold-search` is one of them.
    #[test]
    fn localized_rows_still_carry_a_forwarder() {
        crate::test_utils::init_test_tracing();
        let eval = crate::emacs_core::eval::Context::new();
        let ob = eval.obarray();
        let mut localized = 0usize;
        for var in gnu_table::GNU_OBJECT_VARIABLES {
            let Some(id) = crate::emacs_core::intern::lookup_interned(var.name) else {
                continue;
            };
            let Some(sym) = ob.get_by_id(id) else {
                continue;
            };
            if sym.redirect() != SymbolRedirect::Localized {
                continue;
            }
            localized += 1;
            assert!(
                ob.blv(id).is_some_and(|blv| blv.fwd.is_some()),
                "{} is localized without a forwarder",
                var.name
            );
        }
        assert!(localized > 0, "no localized GNU-declared variable found");
    }
}
