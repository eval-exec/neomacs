#[path = "../sketch.rs"]
mod sketch;
fn wrong(era: &sketch::ProductionEra<'_>, code: sketch::SerialCompiledCode) {
    let _entry = era.enter(code);
}
fn main() {}
