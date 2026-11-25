//! Terminal/TTY display backend.
//!
//! The whole of the terminal writer lives in [`rif`]: the grid, the update
//! planner, the terminal capabilities it consults and the bytes it emits.
//! GNU's equivalent is `src/term.c` plus the `update_frame` half of
//! `src/dispnew.c`, and `rif` is named for the redisplay interface those two
//! sit behind.
//!
//! The rasterizer is a pure function of a sealed presentation: it depends on
//! the display protocol only, so a terminal can be driven without the
//! evaluator or the frontend runtime. What it needs from the evaluator
//! arrives at construction as a [`TtyRasterPolicy`]
//! (`docs/design/display-crate-layout.md`).

// The row rasterizer, and the reference rasterizer its tests compare with,
// thread a row's frame origin, screen position and area layout through one
// call. The display runtime allowed this category crate-wide when it owned
// this code (MILESTONES M4 moved it here); folding the parameters into a
// struct is a separate refactor.
#![allow(clippy::too_many_arguments)]

pub mod raster_policy;
pub mod rif;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use raster_policy::{TextFillEdge, TtyRasterPolicy};
