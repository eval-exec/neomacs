//! Owned row inputs for bounded off-screen workers.
//!
//! Resolved natural text, source-mapped text and literal spacing are admitted.
//! Complete physical-line programs carry geometry, font identity receipts or
//! captured measurements, and work budgets. A bounded worker measures and
//! executes them without evaluator state.
//! Window identity, admission and publication remain the engine's responsibility.

mod input;
pub(crate) use input::ResolvedTextInput;

mod mapped_input;
pub(crate) use mapped_input::ResolvedMappedTextInput;

mod spacing_input;
pub(crate) use spacing_input::ResolvedSpacingInput;

pub(crate) mod program;

pub(crate) mod worker;

pub(crate) mod font_measurement;
