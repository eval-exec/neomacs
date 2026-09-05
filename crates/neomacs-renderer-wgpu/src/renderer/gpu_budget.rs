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
//!   The render pass re-reports every entry from live state once per frame,
//!   which is what keeps a census from drifting: a texture that was freed
//!   reports zero on the next frame rather than staying charged forever.
//!
//! [`SnapshotPool`]: super::snapshot_pool::SnapshotPool

use std::collections::BTreeMap;
use std::num::NonZeroU64;

/// A full-frame GPU allocation the pool does not hand out, and therefore
/// cannot count by construction.
///
/// The variants are exhaustive on purpose: adding a full-frame texture to the
/// render thread should force a decision about which side of the budget it
/// lands on, rather than quietly landing on neither.
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
    unpooled: BTreeMap<(u64, UnpooledTexture), u64>,
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
    pub(crate) fn record_unpooled(&mut self, owner: u64, kind: UnpooledTexture, bytes: u64) {
        let previous = if bytes == 0 {
            self.unpooled.remove(&(owner, kind))
        } else {
            self.unpooled.insert((owner, kind), bytes)
        }
        .unwrap_or(0);
        self.unpooled_bytes = self.unpooled_bytes.saturating_sub(previous) + bytes;
    }

    /// Retire every census entry belonging to a frame window that is gone.
    ///
    /// Without this a destroyed window's atlas and retained scene stay charged
    /// forever, and the budget refuses leases for memory nothing holds.
    pub(crate) fn forget_owner(&mut self, owner: u64) {
        let mut retired = 0u64;
        self.unpooled.retain(|(entry_owner, _), bytes| {
            let keep = *entry_owner != owner;
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
