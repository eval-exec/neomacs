use neovm_core::host_input::HostInputPublisher;

fn extract_sender(publisher: &HostInputPublisher) {
    let _sender = publisher.sender();
}

fn main() {}
