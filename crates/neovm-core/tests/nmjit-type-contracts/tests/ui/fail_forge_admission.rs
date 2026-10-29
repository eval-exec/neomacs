#[path = "../support/sketch.rs"]
mod sketch;
fn main() {
    let mut collector = sketch::ExclusiveCollector::fixture(sketch::CollectorMode::Gen0);
    let _forged = sketch::Gen1Admission { collector: &mut collector };
}
