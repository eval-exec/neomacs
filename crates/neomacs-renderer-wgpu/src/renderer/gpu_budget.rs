//! One ceiling for every full-frame GPU texture the render thread owns.
//!
//! Five kinds of full-frame texture used to be created in four files and
//! freed only on device loss or resize, so "how much VRAM does a frame window
//! cost" had no answer to give. A limit on its own would not have produced
//! one either: the pooled textures are a minority of the bytes. Five
//! full-frame textures at 2560x1440 are about 59 MB, while a single window's
//! glyph atlas can reach 288 MB (8 pages of 2048² across three materials), so
//! a ceiling that saw only the pool would constrain the wrong thing.
//!
//! The budget therefore has two sides, and they differ in how much they can
//! be trusted:
//!
//! * `pooled_bytes` is exact by construction. Only [`SnapshotPool`] moves it,
//!   and a pooled texture cannot be allocated or released anywhere else.
//! * `unpooled_bytes` is a census. It covers full-frame GPU objects whose
//!   owning structs cannot hold a lease, so they report their size instead.
//!   The render pass re-reports each frame window's entries from live state
//!   once per frame, which is what keeps a census from drifting: a texture
//!   that was freed reports zero on the next frame rather than staying charged
//!   forever.
//!
//! The renderer's own stencil clip target is the one census entry that is not
//! re-reported per frame, because it does not need to be: it has a single
//! allocation site, `WgpuRenderer::install_stencil_targets`, which charges it
//! in the same statement that replaces it. Set semantics do the rest — there
//! is nothing for a re-report to correct.
//!
//! [`SnapshotPool`]: super::snapshot_pool::SnapshotPool

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use super::full_frame_texture::FullFrameTexture;

/// Who holds an unpooled full-frame texture.
///
/// Not every one of them belongs to a frame window. The stencil clip target is
/// a single texture the renderer resizes to whichever window it is drawing, so
/// charging it to a window would make it disappear from the census the moment
/// that window closed — while the texture is still allocated, still
/// full-frame sized, and about to be resized for the next window. Naming the
/// two kinds of owner separately is what makes that mistake unrepresentable
/// rather than a convention about which `u64` to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GpuBudgetOwner {
    /// One GUI frame window, keyed by its Emacs frame id.
    FrameWindow(u64),
    /// The renderer itself, for textures shared by every window it draws.
    Renderer,
}

/// A full-frame GPU allocation the pool does not hand out, and therefore
/// cannot count by construction.
///
/// The variants are exhaustive on purpose: adding a full-frame texture to the
/// render thread should force a decision about which side of the budget it
/// lands on, rather than quietly landing on neither. Being exhaustive was not
/// enough on its own — an enum only prompts a decision from someone who
/// happens to look at it — so
/// [`FullFrameTexture::allocate`](super::full_frame_texture::FullFrameTexture::allocate)
/// takes one of these by value and there is no other way to allocate a
/// full-frame texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnpooledTexture {
    /// The cursorless retained scene one frame window blits on compositor-only
    /// frames. Its owning struct still holds a raw texture, so its size is
    /// reported rather than leased.
    RetainedStaticScene,
    /// Every resident glyph-atlas page of one frame window.
    ///
    /// This is also how filled-box cursor cells reach the budget. A retained
    /// cursor cell is a CPU glyph buffer plus a scissor rect and costs no GPU
    /// bytes of its own; what it actually does is pin atlas glyphs, so the
    /// pages it holds resident are counted here and not twice.
    GlyphAtlas,
    /// The renderer's `Stencil8` clip target, used to clip child frames to
    /// rounded corners.
    ///
    /// One texture for the whole renderer, recreated at the size of whichever
    /// window is being drawn — so at a large single window it is full-frame
    /// sized and permanently resident, which is exactly the shape of thing
    /// this budget exists to count. It is a byte per texel rather than four,
    /// which is why it was easy to leave out and why leaving it out was still
    /// a hole: the ceiling is a ceiling, and a megabyte nobody counts is a
    /// megabyte the pool believes it can lease twice.
    StencilClip,
}

/// Why a request for GPU memory was refused.
///
/// Carries the three numbers needed to judge the refusal, because a bare
/// "out of budget" is not evidence: a caller that degrades has to be able to
/// log what it degraded against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExceeded {
    pub requested_bytes: u64,
    pub in_use_bytes: u64,
    pub limit_bytes: NonZeroU64,
}

impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GPU budget exceeded: {} bytes requested with {} in use of {}",
            self.requested_bytes,
            self.in_use_bytes,
            self.limit_bytes.get()
        )
    }
}

impl std::error::Error for BudgetExceeded {}

/// Environment override for the ceiling, in mebibytes.
const LIMIT_ENV: &str = "NEOMACS_GPU_BUDGET_MB";

/// Room for a handful of frame windows at a large size: about 350 MB per
/// window at 2560x1440 with a fully populated atlas.
const DEFAULT_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

/// Accounting for every full-frame GPU texture the render thread owns.
pub struct GpuBudget {
    limit_bytes: NonZeroU64,
    pooled_bytes: u64,
    /// Keyed by owner so a second frame window adds to the census instead of
    /// replacing the first window's entry.
    unpooled: BTreeMap<(GpuBudgetOwner, UnpooledTexture), u64>,
    /// Maintained alongside `unpooled` so a decision costs no map walk.
    unpooled_bytes: u64,
}

impl GpuBudget {
    pub fn new() -> Self {
        let limit = std::env::var(LIMIT_ENV)
            .ok()
            .and_then(|megabytes| megabytes.trim().parse::<u64>().ok())
            .and_then(|megabytes| NonZeroU64::new(megabytes.saturating_mul(1024 * 1024)))
            .unwrap_or_else(|| {
                NonZeroU64::new(DEFAULT_LIMIT_BYTES).expect("non-zero default limit")
            });
        Self::with_limit_bytes(limit)
    }

    pub fn with_limit_bytes(limit_bytes: NonZeroU64) -> Self {
        Self {
            limit_bytes,
            pooled_bytes: 0,
            unpooled: BTreeMap::new(),
            unpooled_bytes: 0,
        }
    }

    pub fn limit_bytes(&self) -> NonZeroU64 {
        self.limit_bytes
    }

    pub fn pooled_bytes(&self) -> u64 {
        self.pooled_bytes
    }

    pub fn unpooled_bytes(&self) -> u64 {
        self.unpooled_bytes
    }

    pub fn in_use_bytes(&self) -> u64 {
        self.pooled_bytes.saturating_add(self.unpooled_bytes)
    }

    /// Charge `bytes` to the pool, or refuse and charge nothing.
    ///
    /// Checking and charging are one operation so that a caller cannot ask
    /// whether a request fits and then charge a different number.
    pub(crate) fn try_charge_pooled(&mut self, bytes: u64) -> Result<(), BudgetExceeded> {
        let in_use = self.in_use_bytes();
        if in_use.saturating_add(bytes) > self.limit_bytes.get() {
            return Err(BudgetExceeded {
                requested_bytes: bytes,
                in_use_bytes: in_use,
                limit_bytes: self.limit_bytes,
            });
        }
        self.pooled_bytes += bytes;
        Ok(())
    }

    pub(crate) fn refund_pooled(&mut self, bytes: u64) {
        self.pooled_bytes = self.pooled_bytes.saturating_sub(bytes);
    }

    /// Report what one unpooled texture currently costs its owner.
    ///
    /// Set semantics, not accumulate: re-reporting the same `(owner, kind)`
    /// replaces the previous figure, so the per-frame re-report cannot inflate
    /// the census, and `0` retires the entry.
    ///
    /// Private to the budget, and deliberately: it is the one signature that
    /// can pair any category with any number, which is exactly the mistake
    /// [`record_full_frame_texture`](Self::record_full_frame_texture) exists to
    /// make unrepresentable. Reporters reach it through the narrow methods
    /// below.
    fn record_unpooled(&mut self, owner: GpuBudgetOwner, kind: UnpooledTexture, bytes: u64) {
        let previous = if bytes == 0 {
            self.unpooled.remove(&(owner, kind))
        } else {
            self.unpooled.insert((owner, kind), bytes)
        }
        .unwrap_or(0);
        self.unpooled_bytes = self.unpooled_bytes.saturating_sub(previous) + bytes;
    }

    /// Report what one full-frame texture currently costs `owner`.
    ///
    /// Both halves of the census entry come from the texture itself, so a
    /// reporter cannot charge a category to the wrong number of bytes, nor
    /// keep charging a size the allocation no longer has.
    pub(crate) fn record_full_frame_texture(
        &mut self,
        owner: GpuBudgetOwner,
        texture: &FullFrameTexture,
    ) {
        self.record_unpooled(owner, texture.kind(), texture.budget_bytes());
    }

    /// Retire `owner`'s census entry for `kind`, which it no longer holds.
    ///
    /// Separate from the report above because an absent texture has no kind to
    /// ask for; naming the category is all this can do, and there is no byte
    /// count to get wrong.
    pub(crate) fn retire_full_frame_texture(
        &mut self,
        owner: GpuBudgetOwner,
        kind: UnpooledTexture,
    ) {
        self.record_unpooled(owner, kind, 0);
    }

    /// Report what every resident glyph-atlas page of one frame window costs.
    ///
    /// The atlas is the one census entry that is not a single texture — it is
    /// a page count that grows and shrinks — so it reports bytes directly
    /// instead of a [`FullFrameTexture`], and gets its own method rather than
    /// a category argument nobody else may pass.
    pub(crate) fn record_glyph_atlas_bytes(&mut self, owner: GpuBudgetOwner, bytes: u64) {
        self.record_unpooled(owner, UnpooledTexture::GlyphAtlas, bytes);
    }

    /// Retire every census entry belonging to a frame window that is gone.
    ///
    /// Without this a destroyed window's atlas and retained scene stay charged
    /// forever, and the budget refuses leases for memory nothing holds.
    ///
    /// Takes a frame id rather than a [`GpuBudgetOwner`] because the
    /// renderer's own entries have no such moment: its textures outlive every
    /// window and are released with the renderer. Being unable to name it here
    /// is the point.
    pub(crate) fn forget_frame_window(&mut self, frame_window: u64) {
        let gone = GpuBudgetOwner::FrameWindow(frame_window);
        let mut retired = 0u64;
        self.unpooled.retain(|(entry_owner, _), bytes| {
            let keep = *entry_owner != gone;
            if !keep {
                retired += *bytes;
            }
            keep
        });
        self.unpooled_bytes = self.unpooled_bytes.saturating_sub(retired);
    }
}

impl Default for GpuBudget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "gpu_budget_test.rs"]
mod tests;
