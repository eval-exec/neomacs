//! Evaluator-thread input methods. Context never crosses into a platform callback.

mod composition;
mod context;
mod export;
mod replacement;
mod request;
mod selection;
mod surrounding;

pub(crate) use context::CompositionState;
pub use request::{ImeEditorError, ImeRequest};
use context::InsertionAnchor;

#[cfg(test)]
mod tests;
