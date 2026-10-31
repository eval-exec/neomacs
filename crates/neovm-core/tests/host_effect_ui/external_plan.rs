use neovm_core::host_effect::contracts;

struct ArbitrarySendPayload(String);

impl<'session> contracts::HostPlan<'session> for ArbitrarySendPayload {
    type Reply = contracts::MessageReply<'session>;
}

fn main() {}
