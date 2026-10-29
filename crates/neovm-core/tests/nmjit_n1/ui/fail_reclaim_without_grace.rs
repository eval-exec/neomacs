#[path = "../sketch.rs"]
mod sketch;
fn wrong(era: &sketch::ProductionEra<'_>) {
    sketch::RetiredTables.reclaim(era);
}
fn main() {}
