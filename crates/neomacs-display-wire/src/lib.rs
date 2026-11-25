//! The NDP/1 display service over the session layer (DESIGN §6.7-§6.11).
//!
//! Message families (`msg`), the one codec (`codec`, bincode 2 `standard()`
//! with deflate), and FrameSync (`present`), which carries a sealed
//! presentation as row-store updates and rebuilds it on the receiver. This
//! crate depends only on the display protocol and its identity leaf: it
//! never touches I/O, threads, the layout engine or the evaluator.

pub mod assets;
pub mod canonical;
pub mod codec;
pub mod msg;
pub mod payload;
pub mod present;
pub mod rows;
pub mod schema;
#[cfg(feature = "testing")]
pub mod shape;
#[cfg(feature = "testing")]
pub mod testing;

pub use codec::{WireCodec, WireError};
pub use payload::Payload;
