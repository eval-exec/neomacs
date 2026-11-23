// A host with nothing to take off its thread, for fixtures whose stand-in
// Context is an `i32`. Included after `use serial_turn::*;`.
impl SerialHost for i32 {
    type Detached = ();
    type Refusal = ();
    fn detach(&mut self, _: HostTransition) -> Result<(), ()> {
        Ok(())
    }
    fn reattach(&mut self, _: (), _: HostTransition) -> Result<(), ((), ())> {
        Ok(())
    }
}
