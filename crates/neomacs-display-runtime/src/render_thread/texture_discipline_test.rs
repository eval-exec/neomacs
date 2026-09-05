//! Guards the rule that the render thread allocates no texture of its own.
//!
//! Every GPU texture the render thread uses is either leased from the snapshot
//! pool (`WgpuRenderer::acquire_snapshot`) or allocated through
//! `FullFrameTexture::allocate`, which cannot be called without naming the
//! `UnpooledTexture` category the allocation is counted under. Both live in
//! `neomacs-renderer-wgpu`, which owns every GPU resource.
//!
//! The retained-static scene used to be the exception: a bare
//! `renderer.device().create_texture(...)` here, with its size re-derived
//! somewhere else once a frame to produce a census figure. That is the shape
//! of thing this guard exists to stop, and unlike the renderer's own guard
//! this one needs no allowlist — the runtime has no legitimate reason to
//! allocate a texture directly, because it does not own GPU resources.

use std::path::{Path, PathBuf};

/// Every way to get a `wgpu::Texture` out of a device.
///
/// `create_texture_bind_group` deliberately does not match: the parenthesis is
/// part of each pattern, so a helper whose name merely starts the same way is
/// not mistaken for an allocation.
const ALLOCATORS: [&str; 3] = [
    "create_texture(",
    "create_texture_with_data(",
    "create_texture_from_hal::",
];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Production `.rs` files under `src/render_thread`, relative to the crate
/// root, with `*_test.rs` and `tests.rs` excluded.
fn render_thread_files() -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = crate_root();
    let mut found = Vec::new();
    walk(&root.join("src/render_thread"), &mut found);
    let mut relative: Vec<String> = found
        .iter()
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            !name.ends_with("_test.rs") && name != "tests.rs"
        })
        .map(|path| {
            path.strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    relative.sort();
    relative
}

#[test]
fn the_render_thread_allocates_every_texture_through_an_accounted_constructor() {
    // If this fails, a texture exists that the GPU budget has never heard of:
    // the runtime allocated it directly instead of through the pool or through
    // FullFrameTexture::allocate, so nothing made it name a census category
    // and the ceiling now covers an unknown fraction of what is resident.
    let root = crate_root();
    let mut raw = Vec::new();

    for file in render_thread_files() {
        let Ok(source) = std::fs::read_to_string(root.join(&file)) else {
            continue;
        };
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if ALLOCATORS.iter().any(|allocator| line.contains(allocator)) {
                raw.push(format!("{file}:{}: {}", index + 1, line.trim()));
            }
        }
    }

    assert!(
        raw.is_empty(),
        "the render thread allocated a texture directly:\n  {}\n\nA texture sized to the frame \
         window belongs in FullFrameTexture::allocate, which makes you name its UnpooledTexture \
         category; one the frame merely composes through belongs in the snapshot pool, via \
         WgpuRenderer::acquire_snapshot.",
        raw.join("\n  ")
    );
}
