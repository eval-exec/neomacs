//! Opt-in deterministic gates in real native paths, absent from default builds.
use std::{path::PathBuf, time::Duration};
fn root() -> Option<PathBuf> {
    std::env::var_os("NEOMACS_GUI_TEST_DIR").map(PathBuf::from)
}
pub fn take(name: &str) -> bool {
    root().is_some_and(|root| std::fs::remove_file(root.join(format!("fault-{name}"))).is_ok())
}
pub fn mark(name: &str) {
    if let Some(root) = root() {
        std::fs::write(root.join(format!("entered-{name}")), b"entered").unwrap();
    }
}
pub fn gate(name: &str, cancelled: impl Fn() -> bool) {
    if !take(name) {
        return;
    }
    let root = root().unwrap();
    mark(name);
    while !root.join(format!("release-{name}")).exists() && !cancelled() {
        std::thread::sleep(Duration::from_millis(5));
    }
}
