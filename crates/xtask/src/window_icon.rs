//! Render the canonical window icon into a macOS `.iconset`.
//!
//! macOS has no SVG icon format: app and volume icons must be `.icns`, which
//! `iconutil` assembles from an `.iconset` directory.  This command produces
//! that directory with the SAME renderer the runtime window icon uses
//! (`neomacs-display-runtime` embeds the identical SVG and rasterizes it with
//! `resvg`), so the bundle icon, the DMG volume icon and the in-app window
//! icon can never be three drifting pictures.
//!
//! Every representation is rendered from the vector source at its exact
//! pixel size; none is a bitmap upscale.

use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;

/// The canonical icon: the exact SVG the runtime embeds for its window icon.
///
/// `crates/neomacs-display-runtime/src/window_icon.rs` `include_bytes!`s this
/// path; the Linux desktop packagers install the same bytes (ledger 195's
/// `packaging must install the exact SVG embedded by the runtime` test).
pub(crate) const CANONICAL_ICON: &str = "crates/neomacs-display-runtime/assets/window-icon.svg";

/// The icon family `iconutil` requires, in its spelling.
const ICONSET: &[(&str, u32)] = &[
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

pub(crate) fn run(
    repo_root: &Path,
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<()> {
    let mut out_dir: Option<PathBuf> = None;
    let mut source: Option<PathBuf> = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "--out-dir" => {
                let value = args.next().ok_or("--out-dir requires a path")?;
                out_dir = Some(PathBuf::from(value));
            }
            "--source" => {
                let value = args.next().ok_or("--source requires a path")?;
                source = Some(PathBuf::from(value));
            }
            other => return Err(format!("render-window-icon: unknown argument: {other}").into()),
        }
    }
    let Some(out_dir) = out_dir else {
        return Err("render-window-icon requires --out-dir DIR".into());
    };
    let source = source.unwrap_or_else(|| repo_root.join(CANONICAL_ICON));
    render_iconset(&source, &out_dir)?;
    println!("rendered macOS iconset at {}", out_dir.display());
    Ok(())
}

fn render_iconset(source: &Path, out_dir: &Path) -> Result<()> {
    let svg = fs::read(source).map_err(|err| format!("{}: {err}", source.display()))?;
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default())
        .map_err(|err| format!("{}: {err}", source.display()))?;
    fs::create_dir_all(out_dir)?;
    for &(name, size) in ICONSET {
        render_png(&tree, size, &out_dir.join(name))?;
    }
    Ok(())
}

/// Rasterize the tree at one exact size and write the PNG.
///
/// Mirrors `neomacs-display-runtime/src/window_icon.rs`'s runtime
/// rasterization: a square pixmap and the vector scaled to fill it.
fn render_png(tree: &resvg::usvg::Tree, size: u32, out: &Path) -> Result<()> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| format!("could not allocate a {size}x{size} pixmap"))?;
    let source = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        size as f32 / source.width(),
        size as f32 / source.height(),
    );
    resvg::render(tree, transform, &mut pixmap.as_mut());
    pixmap
        .save_png(out)
        .map_err(|err| format!("{}: {err}", out.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/window_icon_test.rs"]
mod tests;
