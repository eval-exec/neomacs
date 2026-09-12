//! Evaluator-thread input methods. Context never crosses into a platform callback.

mod composition;
mod context;
mod conversion;
mod export;
mod replacement;
mod request;
mod selection;
mod surrounding;

pub(crate) use context::CompositionState;
use context::InsertionAnchor;
pub use request::{ImeEditorError, ImeRequest};

#[cfg(test)]
mod tests;
