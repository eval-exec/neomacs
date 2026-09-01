//! Terminal/TTY display backend.
//!
//! The whole of the terminal writer lives in [`rif`]: the grid, the update
//! planner, the terminal capabilities it consults and the bytes it emits.
//! GNU's equivalent is `src/term.c` plus the `update_frame` half of
//! `src/dispnew.c`, and `rif` is named for the redisplay interface those two
//! sit behind.

pub mod rif;
