#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::contracts::{HostResult, MessageReply, ReplyPort};
use owner::owner_step::PendingOwnerStep;

fn cannot_settle_terminal_with_intermediate<'s>(
    port: ReplyPort<'s, MessageReply<'s>>,
    pending: PendingOwnerStep<'s, u64>,
) {
    let _ = port.complete(HostResult::Returned(pending));
}
fn main() {}
