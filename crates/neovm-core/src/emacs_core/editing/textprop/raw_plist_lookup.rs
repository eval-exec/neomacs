//! Exact raw plist lookup with one closed-traversal observation selection.

use crate::emacs_core::value::{Value, eq_value};

struct ConsParts {
    car: Value,
    cdr: Value,
}

// Private policy types keep the unobserved entry behind the selector below.
trait ReadPolicy {
    fn parts(cell: Value) -> Option<ConsParts>;
    fn car(cell: Value) -> Value;
    fn cdr(cell: Value) -> Value;
}

struct Observed;
struct Unobserved;

impl ReadPolicy for Observed {
    #[inline(always)]
    fn parts(cell: Value) -> Option<ConsParts> {
        cell.as_cons_ptr().map(|pointer| {
            // SAFETY: as_cons_ptr admits an allocated cons and observes its
            // identity. This closed native lookup invokes no callbacks or GC
            // and retains no pointer beyond these two immediate field reads.
            // The original lookup reads both fields of this key cell as well.
            unsafe {
                ConsParts {
                    car: (*pointer).car.get(),
                    cdr: (*pointer).cdr(),
                }
            }
        })
    }

    #[inline(always)]
    fn car(cell: Value) -> Value {
        cell.cons_car()
    }

    #[inline(always)]
    fn cdr(cell: Value) -> Value {
        cell.cons_cdr()
    }
}

impl ReadPolicy for Unobserved {
    #[inline(always)]
    fn parts(cell: Value) -> Option<ConsParts> {
        if !cell.is_cons() {
            return None;
        }
        Some(ConsParts {
            car: cell.cons_car_unobserved(),
            cdr: cell.cons_cdr_unobserved(),
        })
    }

    #[inline(always)]
    fn car(cell: Value) -> Value {
        cell.cons_car_unobserved()
    }

    #[inline(always)]
    fn cdr(cell: Value) -> Value {
        cell.cons_cdr_unobserved()
    }
}

#[inline]
pub(super) fn get(plist: Value, prop: Value) -> Option<Value> {
    // Empty/default-absent plists retain their old return before any selector.
    if !plist.is_cons() {
        return None;
    }
    if crate::tagged::collection_reads::reads_need_observation() {
        lookup::<Observed>(plist, prop)
    } else {
        // No callbacks, allocation, GC, or Lisp invocation occur in lookup, so
        // this mutator cannot enter a capture during the unobserved traversal.
        lookup::<Unobserved>(plist, prop)
    }
}

#[inline]
fn lookup<P: ReadPolicy>(plist: Value, prop: Value) -> Option<Value> {
    let mut tail = plist;
    loop {
        let ConsParts { car, cdr } = P::parts(tail)?;
        if !cdr.is_cons() {
            return None;
        }
        if eq_value(&car, &prop) {
            return Some(P::car(cdr));
        }
        tail = P::cdr(cdr);
    }
}
