//! W-REG R1 stage 17: core manifest lines 2110-2159 (time-add .. set-time-zone-rule).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "time-add",
            NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_add(args)),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "time-subtract",
            NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_subtract(args)),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "time-less-p",
            NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_less_p(args)),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "time-equal-p",
            NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_equal_p(args)),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "current-time-string",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::timefns::builtin_current_time_string(args)
            }),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "current-time-zone",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::timefns::builtin_current_time_zone(args)
            }),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "encode-time",
            NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_encode_time(args)),
            SubrArity::new(1, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "decode-time",
            NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_decode_time(args)),
            SubrArity::new(0, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "time-convert",
            NativeFn::ContextVec(crate::emacs_core::timefns::builtin_time_convert_in_context),
            SubrArity::new(1, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-time-zone-rule",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::timefns::builtin_set_time_zone_rule(args)
            }),
            SubrArity::new(1, Some(1)),
        )),
}
