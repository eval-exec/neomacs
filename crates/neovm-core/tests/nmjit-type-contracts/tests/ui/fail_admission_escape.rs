#[path = "../support/sketch.rs"]
mod sketch;
fn escape() -> sketch::Gen1Admission<'static> {
    let mut collector = sketch::ExclusiveCollector::fixture(sketch::CollectorMode::Gen1);
    collector.check_gen1().expect("checked GEN1")
}
fn main() {}
