#[path = "../support/sketch.rs"]
mod sketch;
fn main() {
    let mut collector = sketch::ExclusiveCollector::fixture(sketch::CollectorMode::Gen1);
    let admission = collector.check_gen1().expect("checked GEN1");
    let mut era = sketch::ProductionEra::from_admission(admission);
    let stopped = era.stopped();
    sketch::RetiredTables.reclaim(stopped.grace());
}
