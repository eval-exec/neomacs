#[path = "support/wake_owner.rs"]
mod owner;

fn requires_clone<T: Clone>() {}

fn main() {
    requires_clone::<owner::wake::NativePollLease>();
}
