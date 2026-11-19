use neovm_value::error::{InFlightRegistryHandle, detach_source_shell_in_flight};

fn exercise(handle: &InFlightRegistryHandle) {
    let _receipt = detach_source_shell_in_flight(handle);
}

fn main() {
    let _ = exercise;
}
