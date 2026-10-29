//! Cold scalar bridge for retained serial bytecode/native cache witnesses.
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern, intern_uninterned};

struct StampsOn;
impl Drop for StampsOn {
    fn drop(&mut self) {
        crate::emacs_core::symbol::force_fn_stamps_for_test(None);
    }
}
impl Context {
    pub(crate) fn c5_function_stamps_for_test() -> impl Drop {
        crate::emacs_core::symbol::force_fn_stamps_for_test(Some(true));
        StampsOn
    }

    /// Call after all old sources/caches are rooted and warm. Only the registry
    /// grows after input installation; no serial World intern repairs C5.
    pub(crate) fn c5_prepare_intern_growth_for_test(&mut self, prefix: &str) -> [SymId; 2] {
        let fresh = format!("{prefix}-fresh");
        assert!(crate::emacs_core::intern::lookup_interned(&fresh).is_none());
        let holders = [
            intern(&format!("{prefix}-input-left")),
            intern(&format!("{prefix}-input-right")),
        ];
        for id in holders {
            self.obarray
                .set_symbol_value_id(id, crate::emacs_core::value::Value::string(&fresh));
        }
        let mut gap = intern_uninterned("p3sym-c5-serial-cache-gap");
        let target = (u64::from(gap.0) / 4096 + 2) * 4096 + 17;
        while u64::from(gap.0) + 1 < target {
            gap = intern_uninterned("p3sym-c5-serial-cache-gap");
        }
        holders
    }

    pub(crate) fn c5_intern_growth_era_for_test(&mut self, holders: [SymId; 2]) {
        let receipt = self
            .admit_owned_proof_parallel(|projection| {
                // No Context, bytecode borrow, compiled frame or World view enters.
                let result = projection.era().c5_intern_relocating_for_test(holders);
                let (collector, keeper) = projection
                    .finish()
                    .expect("C5 serial cache clean era finish");
                (collector, keeper, result)
            })
            .expect("C5 serial cache owned admission")
            .expect("C5 growth after serial recovery");
        assert!(self.obarray.admission_matches_canonical());
        assert!(receipt.after.chunks >= receipt.before.chunks + 2);
        assert_eq!(receipt.after.chunks, receipt.after.sides);
        assert_ne!(
            receipt.before.spine_address, receipt.after.spine_address,
            "actual pointer-spine relocation required"
        );
        assert_eq!(
            receipt.registry_after,
            (receipt.registry_before.0 + 1, receipt.registry_before.1 + 1)
        );
        eprintln!(
            "P3SYM_C5_SERIAL_GROWTH relocated=1 chunks_added={} registry_delta=1",
            receipt.after.chunks - receipt.before.chunks
        );
    }
}
