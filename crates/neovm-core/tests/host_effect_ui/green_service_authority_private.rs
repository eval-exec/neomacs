//! Green is private: external callers cannot name any owner/service authority.
//! This checks the aggregate privacy barrier, not each inner field in isolation.
use neovm_core::emacs_core::eval::green::host_service::{
    OwnerAcknowledged, RealWaitPermit, RestoredHostOwner,
};
use neovm_core::emacs_core::eval::green::scheduler::ReservedServiceSlice;

fn main() {
    let _: Option<ReservedServiceSlice> = None;
    let _: Option<RealWaitPermit> = None;
    let _: Option<RestoredHostOwner<'static>> = None;
    let _: Option<OwnerAcknowledged<'static>> = None;
}
