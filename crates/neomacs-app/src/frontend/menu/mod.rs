//! Menu hierarchy, panel measurement, and revision-qualified results.
//!
//! Native popup placement and surface lifetime belong to the presentation
//! adapter. Pointer coordinates supplied here are local logical panel pixels.

mod layout;
mod session;

pub use session::{MenuLifetime, MenuSession};
