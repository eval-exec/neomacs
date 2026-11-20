//! W-REG R1: SYSTEM registration stages.
//!
//! Core's C14 compatibility manifest keeps the authoritative sequencing chain
//! and calls {module}::register_subrs(ctx, SubrSegment::Variant) at each
//! recorded position.  Each stage owns one contiguous pre-move run of
//! registrations in a crate-private `define_subrs!(crate_private, ...)` batch,
//! so moving a row exposes no new cross-crate batch symbol.
//!
//! The stage trace (eval test-support) records (crate, segment id, row count)
//! at every call; the core W-REG gate names a missing or extra segment with
//! its crate and row count before the frozen-trace comparison.

use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::subr::SubrBatch;

#[path = "stage01/subrs.rs"]
mod stage01;
#[path = "stage02/subrs.rs"]
mod stage02;
#[path = "stage03/subrs.rs"]
mod stage03;
#[path = "stage04/subrs.rs"]
mod stage04;
#[path = "stage05/subrs.rs"]
mod stage05;
#[path = "stage06/subrs.rs"]
mod stage06;
#[path = "stage07/subrs.rs"]
mod stage07;
#[path = "stage08/subrs.rs"]
mod stage08;
#[path = "stage09/subrs.rs"]
mod stage09;
#[path = "stage10/subrs.rs"]
mod stage10;
#[path = "stage11/subrs.rs"]
mod stage11;
#[path = "stage12/subrs.rs"]
mod stage12;
#[path = "stage13/subrs.rs"]
mod stage13;
#[path = "stage14/subrs.rs"]
mod stage14;
#[path = "stage15/subrs.rs"]
mod stage15;
#[path = "stage16/subrs.rs"]
mod stage16;
#[path = "stage17/subrs.rs"]
mod stage17;

/// Typed stage id for one W-REG SYSTEM registration run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubrSegment {
    Stage01,
    Stage02,
    Stage03,
    Stage04,
    Stage05,
    Stage06,
    Stage07,
    Stage08,
    Stage09,
    Stage10,
    Stage11,
    Stage12,
    Stage13,
    Stage14,
    Stage15,
    Stage16,
    Stage17,
}

/// Ordered stage table: typed id, trace name, batch.
pub(crate) const STARTUP_STAGES: &[(SubrSegment, &str, &SubrBatch)] = &[
    (SubrSegment::Stage01, "system.run01", &stage01::SUBRS),
    (SubrSegment::Stage02, "system.run02", &stage02::SUBRS),
    (SubrSegment::Stage03, "system.run03", &stage03::SUBRS),
    (SubrSegment::Stage04, "system.run04", &stage04::SUBRS),
    (SubrSegment::Stage05, "system.run05", &stage05::SUBRS),
    (SubrSegment::Stage06, "system.run06", &stage06::SUBRS),
    (SubrSegment::Stage07, "system.run07", &stage07::SUBRS),
    (SubrSegment::Stage08, "system.run08", &stage08::SUBRS),
    (SubrSegment::Stage09, "system.run09", &stage09::SUBRS),
    (SubrSegment::Stage10, "system.run10", &stage10::SUBRS),
    (SubrSegment::Stage11, "system.run11", &stage11::SUBRS),
    (SubrSegment::Stage12, "system.run12", &stage12::SUBRS),
    (SubrSegment::Stage13, "system.run13", &stage13::SUBRS),
    (SubrSegment::Stage14, "system.run14", &stage14::SUBRS),
    (SubrSegment::Stage15, "system.run15", &stage15::SUBRS),
    (SubrSegment::Stage16, "system.run16", &stage16::SUBRS),
    (SubrSegment::Stage17, "system.run17", &stage17::SUBRS),
];

/// Install one SYSTEM registration stage at its recorded position in core's
/// sequencing chain.  Records the installed stage for the W-REG trace gate.
pub fn register_subrs(ctx: &mut Context, segment: SubrSegment) {
    let row = STARTUP_STAGES
        .iter()
        .find(|(id, _, _)| *id == segment)
        .copied()
        .expect("every SubrSegment has a startup stage row");
    #[cfg(any(test, feature = "test-support"))]
    neovm_eval::emacs_core::subr::record_registered_subr_stage_for_test(
        "neovm-system",
        row.1,
        row.2.specs().len(),
    );
    row.2.install(ctx);
}

/// Test-support accessor (exactly one per crate): the ordered stage table
/// `(typed id, trace name, batch)`.  Core's test build consumes it for the
/// compiled-batch catalog and the installed-stage trace gate.  This is the
/// VIS move under 5808, not a new public production surface.  The feature
/// gate keeps it out of the production build entirely (a `cfg(test)`-only
/// item is dead code there and would add a new production warning).
#[cfg(feature = "test-support")]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn startup_stages_for_test() -> &'static [(SubrSegment, &'static str, &'static SubrBatch)]
{
    STARTUP_STAGES
}
