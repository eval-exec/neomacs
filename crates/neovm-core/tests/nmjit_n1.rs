//! N1 contracts for proposed types, not yet contracts of the product runtime.
#[path = "nmjit-type-contracts/tests/support/sketch.rs"]
mod sketch;

#[test]
fn nmjit_n1_sketch_gen0_refuses_capability() {
    let mut collector = sketch::ExclusiveCollector::fixture(sketch::CollectorMode::Gen0);
    assert!(matches!(
        collector.check_gen1(),
        Err(sketch::AdmissionError::GenerationalRequired)
    ));
}
#[test]
fn nmjit_n1_sketch_gen1_constructs_once_under_exclusive_borrow() {
    let mut collector = sketch::ExclusiveCollector::fixture(sketch::CollectorMode::Gen1);
    let admission = collector.check_gen1().expect("GEN1 sketch admission");
    let mut era = sketch::ProductionEra::from_admission(admission);
    let stopped = era.stopped();
    sketch::RetiredTables.reclaim(stopped.grace());
}
#[test]
fn nmjit_n1_sketch_types_are_confined() {
    static_assertions::assert_not_impl_any!(sketch::Gen1Admission<'static>: Clone, Copy);
    static_assertions::assert_not_impl_any!(sketch::TableView<'static>: Send, Sync, Clone, Copy);
    static_assertions::assert_not_impl_any!(sketch::JitGracePeriod<'static>: Clone, Copy);
    let _proof = sketch::ProofEra::fixture();
}
