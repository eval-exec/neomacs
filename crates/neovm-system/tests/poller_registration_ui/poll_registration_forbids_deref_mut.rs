//! `PollRegistration<T>` does not implement `DerefMut` and keeps its inner
//! resource private, so safe code cannot `std::mem::replace` a registered
//! descriptor out from under its `Drop` guard. Implementing `PollableResource`
//! requires `unsafe impl`, and `MasterPtyRegistrationToken`'s inner fd is
//! private (unforgeable).
use neovm_eval::emacs_core::eval::data::process_state::{
    MasterPtyRegistrationToken, PollRegistration, PollableResource,
};

struct FakeSource;

impl PollableResource for FakeSource {
    type RegistrationToken = ();

    fn capture_registration_token(&self) {}

    unsafe fn add_to_poller(
        &self,
        _poller: &polling::Poller,
        _token: (),
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    unsafe fn modify_on_poller(
        &self,
        _poller: &polling::Poller,
        _token: (),
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    unsafe fn unregister_from_poller(&self, _poller: &polling::Poller, _token: ()) {}
}

struct UnitSource;

// SAFETY: unit test stub with no OS descriptors.
unsafe impl PollableResource for UnitSource {
    type RegistrationToken = ();

    fn capture_registration_token(&self) {}

    unsafe fn add_to_poller(
        &self,
        _poller: &polling::Poller,
        _token: (),
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    unsafe fn modify_on_poller(
        &self,
        _poller: &polling::Poller,
        _token: (),
        _event: polling::Event,
        _mode: polling::PollMode,
    ) -> std::io::Result<()> {
        Ok(())
    }

    unsafe fn unregister_from_poller(&self, _poller: &polling::Poller, _token: ()) {}
}

fn read_private_fields(reg: &PollRegistration<UnitSource>) {
    let _ = &reg.inner;
    let _ = &reg.poller;
}

fn forge_master_pty_token() {
    let _ = MasterPtyRegistrationToken(Some(9_999_999));
    let _ = <MasterPtyRegistrationToken as Default>::default();
}

fn swap_inner(reg: &mut PollRegistration<std::fs::File>, replacement: std::fs::File) {
    let _ = std::mem::replace(&mut *reg, replacement);
}

fn main() {
    let _ = read_private_fields;
    let _ = forge_master_pty_token;
    let _ = swap_inner;
}
