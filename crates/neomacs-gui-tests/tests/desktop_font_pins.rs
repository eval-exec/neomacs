//! Pinned font catalog for the desktop-font suites.
//!
//! The desktop-font fixtures capture pixel grids from the Linux GUI CI job's
//! font packages ("Ubuntu Mono 13" is a 10x20 cell there, but a 9x18 cell on
//! the pre-0.869 Ubuntu Mono some hosts still carry, and fontconfig 2.18
//! ranks Noto above DejaVu while the runner's 2.14 ranks DejaVu first).
//! Every run of these suites therefore points `FONTCONFIG_FILE` at a config
//! whose only `<dir>` is the hash-verified fixture directory, for the GNU
//! reference and Neomacs alike, so the faces — and the cells they realize —
//! are the same on every host.

use std::path::{Path, PathBuf};

/// Write the process-local fontconfig for one desktop-font scenario.
///
/// The returned file has no system directories and no `<include>`: face
/// selection can only see the pinned Ubuntu 0.869 and DejaVu 2.37 files.
#[must_use]
pub(crate) fn desktop_font_fontconfig(artifacts: &Path) -> PathBuf {
    let fonts = neomacs_test_fonts::desktop_fonts().directory();
    let cache = artifacts.join("desktop-font-fontconfig-cache");
    std::fs::create_dir_all(&cache).expect("create the desktop-font fontconfig cache");
    let config = artifacts.join("desktop-fonts.conf");
    std::fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            fonts.display(),
            cache.display()
        ),
    )
    .expect("write the pinned desktop-font fontconfig");
    config
}
