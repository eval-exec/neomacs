//! Compiler-owned scalar array-kind snapshot and admission hints.
//! All masks/hints are owned Rust scalars captured on the mutator before worker
//! dispatch. This TLS is compiler scratch, not a Lisp-state/runtime cache; nested
//! compiles restore the previous vector and two compiler threads never share it.
//! A background job receives the owned FeedbackSnapshot through its existing
//! front payload. No worker calls Value::kind or reads an ArraySite table.

use super::*;
use crate::emacs_core::jit::feedback::arrays::{ArrayKindMask, ObservedArrayKind, PlainArrayKind};
use crate::emacs_core::jit::opt::passes::array_reads::ArrayAdmission;
use crate::emacs_core::jit::opt::{ir, types::TypeSet};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static ACTIVE_ARRAY_KINDS: RefCell<Vec<ArrayKindMask>> = const { RefCell::new(Vec::new()) };
}

pub(super) struct ArrayFeedbackScope(Option<Vec<ArrayKindMask>>);
impl ArrayFeedbackScope {
    pub(super) fn enter(kinds: Vec<ArrayKindMask>) -> Self {
        Self(Some(
            ACTIVE_ARRAY_KINDS.with(|v| std::mem::replace(&mut *v.borrow_mut(), kinds)),
        ))
    }
}
impl Drop for ArrayFeedbackScope {
    fn drop(&mut self) {
        if let Some(prev) = self.0.take() {
            ACTIVE_ARRAY_KINDS.with(|v| *v.borrow_mut() = prev);
        }
    }
}

pub(crate) fn selected() -> bool {
    jit_opt_mode() == OptMode::Opt && jit_opt_passes().range
}

/// Fused caller instructions map to the original source pc. A spliced callee
/// region has no recorded site in that caller and deliberately returns bottom.
/// This bounded first observer does not invent an array profile for an inlined
/// callee from its call-site pc or consult the callee's Lisp object on a worker.
pub(super) fn active_kind(pc: usize) -> Option<PlainArrayKind> {
    let pc = match inline::active_fused() {
        Some(fused) if fused.region_at(pc).is_some() => return None,
        Some(fused) => fused.caller_pc(pc)?,
        None => pc,
    };
    ACTIVE_ARRAY_KINDS
        .with(|v| v.borrow().get(pc).copied())
        .and_then(ArrayKindMask::plain)
}

fn type_of(kind: PlainArrayKind) -> TypeSet {
    match kind {
        PlainArrayKind::Vector => TypeSet::VECTOR,
        PlainArrayKind::Record => TypeSet::RECORD,
        PlainArrayKind::VectorOrRecord => TypeSet::VECTOR.join(TypeSet::RECORD),
    }
}

/// Call this on the FRONT while the FINAL fused pool is rooted. The function
/// selects admission hints only: the resulting plan always guards the shape.
/// Original dynamic-prefix constants are excluded; Fused v2 rejects callee
/// patched prefixes already. The owned table can then cross to a worker.
pub(super) fn admission(ops_len: usize, constants: &[Value], prefix: usize) -> ArrayAdmission {
    let site_types = (0..ops_len)
        .map(|pc| active_kind(pc).map_or(TypeSet::BOTTOM, type_of))
        .collect();
    let constant_types = constants
        .iter()
        .enumerate()
        .skip(prefix)
        .filter_map(|(index, &value)| {
            let ty = match ObservedArrayKind::of(value) {
                ObservedArrayKind::PlainVector => TypeSet::VECTOR,
                ObservedArrayKind::PlainRecord => TypeSet::RECORD,
                ObservedArrayKind::Other => return None,
            };
            Some((index as u32, (ir::ValueBits::from_value(value), ty)))
        })
        .collect::<HashMap<_, _>>();
    ArrayAdmission {
        site_types,
        constant_types,
    }
}
