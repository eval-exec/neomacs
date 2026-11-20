//! Dormant branded custody. No production caller enters these types.
//! The unentered reservation is dormant pending keeper commits 30–37, which
//! may reshape it without a compatibility burden.
#![forbid(unsafe_code)]

mod published_owner;
mod reservation;
mod stop_scope;

#[doc(hidden)]
pub use published_owner::PublishedOwner;
#[doc(hidden)]
pub use reservation::UnenteredReservation;
#[doc(hidden)]
pub use stop_scope::{StopScope, StoppedInterval, with_stop_scope};
