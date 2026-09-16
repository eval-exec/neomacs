//! Product font assets from the authenticated browser package bundle.

use std::sync::Arc;

use neomacs_display_protocol::font::FontMemoryAsset;
use neomacs_layout_engine::font_backend::install_packaged_fonts;
use neovm_core::emacs_core::fileio::RuntimeResourceStore;

pub(crate) fn initialize(resources: &dyn RuntimeResourceStore) -> Result<(), String> {
    let path = resources
        .mount_root()
        .join("lisp/neomacs-wasm-packages/nerd-icons/fonts/NFM.ttf");
    let mut assets = Vec::new();
    // Packages are optional: core startup must still work if their download failed.
    if let Some(bytes) = resources.file_contents(&path) {
        assets.push(
            FontMemoryAsset::new("packaged:nerd-icons/NFM.ttf#0", Arc::new(bytes.to_vec()), 0)
                .ok_or("empty packaged Nerd Icons font")?,
        );
    }
    install_packaged_fonts(assets).map_err(str::to_owned)
}
