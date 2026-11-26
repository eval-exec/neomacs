//! Pins the landed product edges the safety-gate MODELS copy.
//! A moved or deleted edge fails this test. This is not a product execution.
use std::path::Path;

fn pin(relative: &str, needles: &[&str]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("seam read {}: {error}", path.display()));
    for needle in needles {
        assert!(text.contains(needle), "seam lost `{needle}` in {relative}");
    }
    println!("PENTRY_SEAM file={relative} anchors={}", needles.len());
}

#[test]
fn safety_gate_seams_pin_landed_product_edges() {
    pin(
        "../../crates/neovm-value/src/tagged/gc/mutator_registration/rendezvous/keeper.rs",
        &[
            "A single keeper cannot wait for its own outstanding guard.",
            "Phase::Tracing(_) => return Err(RendezvousError::Busy),",
            "state.owner_thread == Some(registry.keeper_thread)",
            "super::Mode::Running | super::Mode::Publishing",
            "verify_stopped(registry, &gate)?;",
            "requires every actual root/image/loan/epoch witness to match.",
        ],
    );
    pin(
        "../../crates/neovm-value/src/tagged/gc/mutator_registration/world_loan.rs",
        &[
            "The word is not a pointer permission.",
            "`into_retained_owner` transfers that same unique owner",
            "`adopt_current_thread` records the calling thread as the",
        ],
    );
    pin(
        "../../crates/neovm-value/src/tagged/gc/facade_mark/heap_transfer.rs",
        &[
            "Forgetting either must not destroy or reactivate the shell.",
            "Its field Drop then poisons and retains unfinished backing.",
            "Every enclosing source frame with a reference-bearing collector or era",
        ],
    );
}
