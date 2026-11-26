use neovm_value::tagged::gc::heap_transfer::UnenteredReservation;

fn main() {
    let _reservation: UnenteredReservation<'static, 'static> = UnenteredReservation::new();
}
