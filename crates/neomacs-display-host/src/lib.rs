//! The evaluator side of a Neomacs display (`docs/design/display-crate-layout.md`).
//!
//! What the evaluator thread does for a display lives here, not in the
//! frontend runtime: the redisplay runtime that lays frames out and answers
//! GNU's synchronous display queries, and the evaluator policies a terminal
//! rasterizer is constructed with. The runtime and the renderer never link
//! the Lisp machine (MILESTONES M4); later milestones add the display link
//! itself (terminals of type `neo`, the outbox and request correlation).

pub mod redisplay;
mod redisplay_step;
pub mod tty_raster_policy;

#[cfg(test)]
mod tests;
