#[path = "support/wake_owner.rs"]
mod owner;

fn requires_send<T: Send>() {}

fn main() {
    requires_send::<owner::wake::native_wait::RealWaitPermit>();
}
