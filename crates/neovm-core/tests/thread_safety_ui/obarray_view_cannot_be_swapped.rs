//! RB-B1: the exclusive obarray-view capability is not swappable.
//!
//! `ObarrayViewMut` implements `Deref` but deliberately no `DerefMut`, so safe
//! code cannot obtain `&mut ObarrayView`. Before the capability existed, two
//! `&mut ObarrayView` values could be `mem::swap`ped, exchanging the manager,
//! dispatch and lease pointers of two Contexts and cross-linking their Worlds
//! (UB). This fixture proves the swap is rejected.

use neovm_core::emacs_core::eval::ObarrayViewMut;

fn swap_views(first: ObarrayViewMut<'_>, second: ObarrayViewMut<'_>) {
    std::mem::swap(&mut *first, &mut *second);
}

fn main() {}
