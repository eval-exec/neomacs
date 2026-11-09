//! Face-attribute registry state (stage-4 K2 oracleb stolen slice, E5-FT).
//!
//! The face attribute state payload, its handle alias and the pure GC-root
//! collection moved here from display/xfaces.  The display layer keeps the
//! registry thread-local, the realization/remapping logic and the carrier
//! park/fingerprint hooks (published through the eval lifecycle table).

use neovm_value::display_vocab::LFaceAttr;
use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::intern::SymId;
use neovm_value::value::Value;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub const FIRST_DYNAMIC_FACE_ID: i64 = 186;

pub struct FaceAttrState {
    pub selected_created: FxHashSet<SymId>,
    pub selected_overrides: FxHashMap<SymId, FxHashMap<LFaceAttr, Value>>,
    pub defaults_overrides: FxHashMap<SymId, FxHashMap<LFaceAttr, Value>>,
    pub created_lisp_faces: FxHashSet<SymId>,
    pub created_face_ids: FxHashMap<SymId, i64>,
    pub next_created_face_id: i64,
    pub face_set_generation: u64,
    // The sorted name cache follows the same Context as its face definitions.
    pub face_name_list_cache: Option<(u64, Arc<[String]>)>,
}

impl Default for FaceAttrState {
    fn default() -> Self {
        Self {
            selected_created: FxHashSet::default(),
            selected_overrides: FxHashMap::default(),
            defaults_overrides: FxHashMap::default(),
            created_lisp_faces: FxHashSet::default(),
            created_face_ids: FxHashMap::default(),
            next_created_face_id: FIRST_DYNAMIC_FACE_ID,
            face_set_generation: 0,
            face_name_list_cache: None,
        }
    }
}

pub type FontRegistryHandle = HeapRegistryHandle<FaceAttrState>;

/// Collect GC roots from the collecting Context's face attribute overrides.
pub fn collect_font_registry_gc_roots(registry: &FontRegistryHandle, roots: &mut Vec<Value>) {
    let state = registry.borrow();
    for attrs in state.selected_overrides.values() {
        roots.extend(attrs.values().copied());
    }
    for attrs in state.defaults_overrides.values() {
        roots.extend(attrs.values().copied());
    }
}
