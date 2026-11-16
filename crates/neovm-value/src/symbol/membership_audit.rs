//! PEER 5786 startup census: every Lisp-visible canonical symbol is a member.
//!
//! GNU interns every name it hands to Lisp through `intern`
//! (`src/lread.c:4674-4709`) and creates an uninterned object only through
//! `make-symbol` (`src/alloc.c:3660-3711`). A canonical `SymId` that carries
//! a function cell, a bound value cell or a plist, or that appears inside
//! another symbol's cells or a startup variable, but is not a member of the
//! obarray, is the port's silent-visibility bug class: Lisp can observe the
//! symbol yet `(intern-soft (symbol-name sym))` answers nil.
//!
//! This is a dedicated test-support census, not a per-Context cost: the
//! suite creates thousands of Contexts and a per-Context walk would tax all
//! of them (lead #6267). Call it on a fresh Context and on a
//! pdump-restored one.
//!
//! Symbols reachable through conses and vectors are walked. Symbols hidden
//! inside strings, hash tables or char-tables are not; a startup surface
//! that puts a fresh canonical symbol only inside one of those would need
//! its own row.

use std::collections::HashSet;

use super::Obarray;
use crate::intern::{SymId, is_canonical_id, is_gnu_startup_uninterned_spelling, resolve_sym};
use crate::tagged::value::SymbolMode;
use crate::value::Value;

/// One canonical symbol Lisp can observe that is not an obarray member.
///
/// `holder` names the member whose function cell, value cell or plist made
/// the symbol reachable; it is the first such holder for a deduplicated
/// name and makes a failing census actionable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibilityViolation {
    pub name: String,
    pub holder: String,
}

impl Obarray {
    /// Every canonical symbol a startup Context can observe must be an
    /// obarray member, except GNU's documented startup-uninterned spellings
    /// (`neovm_value::intern::is_gnu_startup_uninterned_spelling`).
    ///
    /// Non-canonical ids are GNU `make-symbol` objects and are exempt by
    /// kind. The result is sorted by name and deduplicated.
    #[doc(hidden)]
    pub fn audit_startup_lisp_visibility(&self) -> Vec<VisibilityViolation> {
        let mut violations = Vec::new();
        let mut seen = HashSet::new();
        // The obarray's Lisp-visible surface is the cells of its MEMBERS.
        // A non-member slot can hold Rust-side scratch cells, but nothing
        // reachable from the obarray exposes them; only a symbol that is a
        // value inside a member's cells can leak to Lisp.
        for id in self.global_member_ids() {
            let holder = resolve_sym(id).to_owned();
            if let Some(function) = self.symbol_function_id(id) {
                self.walk_visible_value(function, &holder, &mut seen, &mut violations);
            }
            if let Some(value) = self.symbol_value_id_copied(id) {
                self.walk_visible_value(value, &holder, &mut seen, &mut violations);
            }
            let plist = self.symbol_plist_id(id);
            if !plist.is_nil() {
                self.walk_visible_value(plist, &holder, &mut seen, &mut violations);
            }
        }
        violations.sort_by(|left, right| left.name.cmp(&right.name));
        violations.dedup_by(|left, right| left.name == right.name);
        violations
    }

    fn check_visible_symbol(
        &self,
        id: SymId,
        holder: &str,
        violations: &mut Vec<VisibilityViolation>,
    ) {
        // GNU make-symbol objects are uninterned by kind and legitimate.
        if !is_canonical_id(id) || self.is_global_member(id) {
            return;
        }
        let name = resolve_sym(id);
        // GNU's own startup-uninterned spellings have a source contract
        // (the five Funinterns plus Qunbound); the shared generated list is
        // the single allowlist.
        if is_gnu_startup_uninterned_spelling(name) {
            return;
        }
        violations.push(VisibilityViolation {
            name: name.to_owned(),
            holder: holder.to_owned(),
        });
    }

    fn walk_visible_value(
        &self,
        root: Value,
        holder: &str,
        seen: &mut HashSet<usize>,
        violations: &mut Vec<VisibilityViolation>,
    ) {
        let mut stack = vec![root];
        while let Some(value) = stack.pop() {
            // `bits` covers conses, vectors and immediates alike; cycles
            // terminate because every value is visited once.
            if !seen.insert(value.bits()) {
                continue;
            }
            if let Some(id) = value.as_symbol_id(SymbolMode::Bare) {
                self.check_visible_symbol(id, holder, violations);
                continue;
            }
            if value.is_cons() {
                stack.push(value.cons_car());
                stack.push(value.cons_cdr());
            } else if let Some(data) = value.as_vector_data() {
                stack.extend(data.as_slice().iter().copied());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Obarray;
    use crate::intern::intern;
    use crate::value::Value;

    /// The census must catch a canonical symbol that is reachable from
    /// another symbol's value cell but was never interned, and must accept
    /// the same name once it goes through the GNU intern funnel.
    #[test]
    fn reachable_canonical_non_member_is_reported_until_interned() {
        let mut obarray = Obarray::new();
        obarray.intern_member("p5786-census-holder");
        let planted = intern("p5786-planted-nonmember");
        let list = Value::cons(Value::from_sym_id(planted), Value::NIL);
        obarray.set_symbol_value("p5786-census-holder", list);

        let violations = obarray.audit_startup_lisp_visibility();
        assert!(
            violations
                .iter()
                .any(|violation| violation.name == "p5786-planted-nonmember"),
            "the planted non-member must be reported: {violations:?}"
        );

        obarray.intern_member("p5786-planted-nonmember");
        let violations = obarray.audit_startup_lisp_visibility();
        assert!(
            !violations
                .iter()
                .any(|violation| violation.name == "p5786-planted-nonmember"),
            "the intern funnel must clear the violation: {violations:?}"
        );
    }
}
