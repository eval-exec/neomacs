//! `PollRegistration<T>` does not implement `DerefMut` and keeps its inner
//! resource private, so safe code cannot `std::mem::replace` a registered
//! descriptor out from under its `Drop` guard. Implementing `PollableResource`
//! also requires `unsafe impl`.
use neovm_eval::emacs_core::eval::data::process_state::{PollRegistration, PollableResource};

struct FakeSource;

impl PollableResource for FakeSource {
    unsafe fn add_to_poller(
        &self,
        _poller: &polling::Poller,
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    fn modify_on_poller(
        &self,
        _poller: &polling::Poller,
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    fn unregister_from_poller(&self, _poller: &polling::Poller) {}
}

struct UnitSource;

// SAFETY: unit test stub with no OS descriptors.
unsafe impl PollableResource for UnitSource {
    unsafe fn add_to_poller(
        &self,
        _poller: &polling::Poller,
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    fn modify_on_poller(
        &self,
        _poller: &polling::Poller,
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    fn unregister_from_poller(&self, _poller: &polling::Poller) {}
}

fn read_private_fields(reg: &PollRegistration<UnitSource>) {
    let _ = &reg.inner;
    let _ = &reg.poller;
}

fn swap_inner(reg: &mut PollRegistration<std::fs::File>, replacement: std::fs::File) {
    let _ = std::mem::replace(&mut *reg, replacement);
}

fn main() {
    let _ = read_private_fields;
    let _ = swap_inner;
}
