use neovm_core::emacs_core::Context;
use neovm_core::host_input::{HostInputPublishError, HostInputPublisher};
use neovm_core::keyboard::InputEvent;

fn assert_publisher_traits<T: Send + Sync + Clone + std::fmt::Debug>() {}

fn publish_through_owner(context: &mut Context) -> Result<(), HostInputPublishError> {
    let publisher = HostInputPublisher::install(context);
    let producer = publisher.clone();
    context.init_input_system();
    producer.send(InputEvent::LayoutInvalidated)?;
    publisher.send_batch(std::iter::empty())?;
    Ok(())
}

fn main() {
    assert_publisher_traits::<HostInputPublisher>();
    let _ = publish_through_owner;
}
