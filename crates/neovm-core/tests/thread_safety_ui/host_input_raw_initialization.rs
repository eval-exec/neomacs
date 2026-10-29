use neovm_core::emacs_core::Context;
use neovm_core::keyboard::InputEvent;

fn activate_raw(context: &mut Context, receiver: crossbeam_channel::Receiver<InputEvent>) {
    context.init_input_system(receiver);
}

fn main() {}
