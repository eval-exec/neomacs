#[path = "../support/sketch.rs"]
mod sketch;
fn escape() -> sketch::TableView<'static> {
    let entries = vec![sketch::ChunkSlot(0)];
    sketch::TableView::fixture(&entries)
}
fn main() {}
