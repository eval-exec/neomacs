use neovm_core::emacs_core::forward::LispBufferObjFwd;

// A per-buffer forwarder's raw slot offset is private to the forward module:
// a thread projection's header carries a marker no buffer has, so a slot is
// reachable only through the checked accessors.
fn raw_offset(fwd: &LispBufferObjFwd) -> u16 {
    fwd.offset
}

fn main() {
    let _ = raw_offset;
}
