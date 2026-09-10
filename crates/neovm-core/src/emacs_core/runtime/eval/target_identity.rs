//! Target-owned Lisp identity at bootstrap and portable-image restoration.

use super::{Context, Obarray, Value, gnu_system_type, initial_feature_ids, intern};
use std::collections::HashSet;

/// Install GNU's target-width fixnum limit constants (`src/data.c`).
///
/// These values cannot come from `i64`'s width: a portable image produced on
/// a 64-bit host is also restored by wasm32, whose immediate fixnums have a
/// different range. Keep construction behind this one target-compiled seam
/// so bootstrap, test harnesses, and post-image normalization cannot diverge.
pub(crate) fn install_fixnum_limit_variables(obarray: &mut Obarray) {
    for (name, limit) in [
        ("most-positive-fixnum", Value::MOST_POSITIVE_FIXNUM),
        ("most-negative-fixnum", Value::MOST_NEGATIVE_FIXNUM),
    ] {
        obarray.set_symbol_value(name, Value::fixnum(limit));
        obarray.make_special(name);
        obarray.set_constant(name);
    }
}

impl Context {
    /// Rebind target-owned Lisp identity after restoring a portable image.
    ///
    /// A portable snapshot carries editor state, not the producer binary's
    /// platform contract. This consumer therefore owns `system-type` and the
    /// C-level feature tail, just as its compiled primitive catalog owns the
    /// callable subr surface.
    pub(crate) fn rebind_compiled_target_identity(&mut self) {
        self.set_variable("system-type", Value::symbol(gnu_system_type()));

        let target_owned_features = super::super::c_features::gnu_c_features()
            .into_iter()
            .map(|feature| intern(feature.name))
            .collect::<HashSet<_>>();
        self.features
            .retain(|feature| !target_owned_features.contains(feature));
        self.features.extend(initial_feature_ids());
        self.sync_features_variable();
    }
}
