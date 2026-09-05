//! Guards the rule that every GPU texture allocation states which budget it
//! belongs to.
//!
//! Two budgets bound this crate's textures and they bound different things.
//! `GpuBudget` bounds the full-frame textures whose size follows a frame
//! window — the ones that cost tens of megabytes each and are resident for as
//! long as the window is. `media_budget` bounds the content-sized ones —
//! decoded images, video frames, WebView surfaces, imported DMA-BUFs, atlas
//! pages — whose size follows what is being displayed rather than how big the
//! window is.
//!
//! `FullFrameTexture::allocate` makes the first kind impossible to allocate
//! without naming a census category. What it cannot do is stop the next
//! full-frame texture from being written as a bare `device.create_texture`
//! that never reaches it, which is precisely how the retained-static scene and
//! the stencil clip target came to be uncounted in the first place.
//!
//! So this test enumerates every raw allocation left in the crate and demands
//! a reason for each. Adding one fails the build until it is listed here, and
//! the only justification that passes review is "this texture is sized by
//! content, not by the window" — because a window-sized one has a constructor
//! to go through.
//!
//! Modelled on `neomacs-display-runtime`'s `time_discipline_test`, which does
//! the same job for raw clock reads and for the same reason: the wrong thing
//! is easy to write and hard to notice in review.

use std::collections::BTreeMap;
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

/// Every sanctioned raw texture allocation, as `file` -> why it is allowed.
///
/// Keep the justifications specific. "Not full-frame" is not a justification;
/// the entry should say what determines the texture's size and which budget
/// bounds it.
fn allowlist() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        (
            "src/renderer/full_frame_texture.rs",
            "The accounted full-frame constructor itself. Every window-sized \
             texture in the render thread is allocated here, under an \
             UnpooledTexture category the caller has to name.",
        ),
        (
            "src/renderer/snapshot_pool.rs",
            "Pooled full-frame slots, counted exactly by construction: the \
             pool charges GpuBudget::try_charge_pooled before `create` runs \
             and refunds on release, so these are the one kind the census \
             does not have to re-report.",
        ),
        (
            "src/glyph_atlas/pages.rs",
            "Atlas pages are page_size square, fixed at atlas construction and \
             independent of the window. They are counted as a page census \
             under UnpooledTexture::GlyphAtlas, reported per frame from \
             WgpuGlyphAtlas::resident_bytes.",
        ),
        (
            "src/image_cache.rs",
            "Sized to the decoded raster, bounded by media_budget's Image \
             accounting and the cache's own LRU.",
        ),
        (
            "src/video_cache.rs",
            "Sized to the decoded video frame, bounded by media_budget's Video \
             accounting.",
        ),
        (
            "src/webview_cache.rs",
            "Sized to the WPE browser frame, bounded by media_budget's WebKit \
             accounting.",
        ),
        (
            "src/shader_surface_cache.rs",
            "Sized to the shader surface's own clamped extent (MAX_SURFACE_SIZE), \
             bounded by media_budget's Surface accounting and its eviction driver.",
        ),
        (
            "src/external_buffer.rs",
            "Sized to the imported platform buffer, whose extent comes from the \
             producer; it backs one media surface and is charged with it.",
        ),
        (
            "src/vulkan_dmabuf.rs",
            "Sized to the imported DMA-BUF, whose extent comes from the \
             producer (a WPE view or a video decoder); it backs one media \
             surface and is charged with it.",
        ),
    ])
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Production `.rs` files in this crate, relative to the crate root, with
/// `*_test.rs` and `tests.rs` excluded.
fn production_files() -> Vec<String> {
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
    walk(&root.join("src"), &mut found);
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

/// Lines that allocate a texture, ignoring comments and doc comments.
fn texture_allocations(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
                && ALLOCATORS.iter().any(|allocator| line.contains(allocator))
        })
        .map(|(index, line)| (index + 1, line.trim().to_string()))
        .collect()
}

#[test]
fn every_raw_texture_allocation_in_the_renderer_states_which_budget_bounds_it() {
    // If this fails, a texture was allocated without anyone deciding whether
    // it scales with the window or with the content — and the GPU budget goes
    // back to being a ceiling over an unknown fraction of the memory the
    // render thread actually holds, which is the state that made it possible
    // to leave five full-frame textures uncounted.
    let allowed = allowlist();
    let root = crate_root();
    let mut unjustified = Vec::new();

    for file in production_files() {
        let Ok(source) = std::fs::read_to_string(root.join(&file)) else {
            continue;
        };
        let allocations = texture_allocations(&source);
        if allocations.is_empty() || allowed.contains_key(file.as_str()) {
            continue;
        }
        for (line, text) in allocations {
            unjustified.push(format!("{file}:{line}: {text}"));
        }
    }

    assert!(
        unjustified.is_empty(),
        "raw texture allocations with no stated budget:\n  {}\n\nA texture sized to the frame \
         window belongs in FullFrameTexture::allocate, which makes you name its UnpooledTexture \
         category. A content-sized one belongs in the allowlist in this file, with a note saying \
         what determines its size and which budget bounds it.",
        unjustified.join("\n  ")
    );
}

#[test]
fn no_allowlisted_file_has_stopped_allocating_textures() {
    // If this fails, the allowlist is describing allocations that no longer
    // exist, which is how a guard rots into a list nobody trusts: the next
    // person to add a texture to that file inherits a justification written
    // for something else.
    let root = crate_root();
    let stale: Vec<&str> = allowlist()
        .keys()
        .copied()
        .filter(|file| {
            std::fs::read_to_string(root.join(file))
                .is_ok_and(|source| texture_allocations(&source).is_empty())
        })
        .collect();

    assert!(
        stale.is_empty(),
        "allowlisted files that no longer allocate a texture: {stale:?}"
    );
}
