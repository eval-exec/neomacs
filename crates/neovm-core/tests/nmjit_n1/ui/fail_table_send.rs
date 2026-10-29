#[path = "../sketch.rs"]
mod sketch;
fn require_send<T: Send>() {}
fn main() {
    require_send::<sketch::TableView<'static>>();
}
