//! Android input-method adapter. Editor authority remains on the VM thread.

mod synchronization;

pub(super) use synchronization::InputMethod;

#[cfg(test)]
mod tests;
