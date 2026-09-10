//! Evaluator-thread input methods. Context never crosses into a platform callback.

mod composition;
mod context;
mod selection;
mod surrounding;

pub(crate) use context::CompositionState;
use context::InsertionAnchor;

#[cfg(test)]
mod tests;
