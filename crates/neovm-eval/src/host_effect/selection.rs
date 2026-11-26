//! Typed native payloads for the five selection primitives.
//!
//! A selection call is evaluated by its calling Lisp thread: arity,
//! argument conversion and the Lisp result are decided there, exactly as GNU
//! runs the call on the thread that made it. Only these owned values cross to
//! the owner that holds the display host; no `Value`, root or evaluator
//! borrow crosses. The reply is a value or a host error; a transport failure
//! is not represented here.
//!
//! The types are defined in [`super::contracts`] so the path-included
//! host-effect compile fixtures can name them; this module is the stable
//! production import path.

pub use super::contracts::{
    SelectionHostReply as SelectionReply, SelectionOwnerReply, SelectionRequest,
};
