use neovm_core::emacs_core::Context;
use neovm_core::keyboard::InputEvent;

fn attach_raw(context: &mut Context, receiver: crossbeam_channel::Receiver<InputEvent>) {
    context.input_rx = Some(receiver);
}

fn main() {}
