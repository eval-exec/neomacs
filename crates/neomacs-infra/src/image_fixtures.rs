//! Image fixture pins shared by explicit prefetch and cache-only test reads.
//!
//! `cargo xtask image-fixtures` prepares these files in the existing pinned
//! fixture cache. Tests verify cached bytes without fetching on a miss.

pub const NYAN_MODE_NAME: &str = "nyan-mode-nyan.xpm";
pub const NYAN_MODE_COMMIT: &str = "09904af23adb839c6a9c1175349a1fb67f5b4370";
pub const NYAN_MODE_URL: &str = "https://raw.githubusercontent.com/TeMPOraL/nyan-mode/09904af23adb839c6a9c1175349a1fb67f5b4370/img/nyan.xpm";
pub const NYAN_MODE_SHA256: &str =
    "d80164170278176347b1c01abc70c552ca2fcb655e279bea9f369206eb2d415e";
