// The production table is public, but only the owning World may issue a
// descriptor and register its canonical plain root with the obarray.
use neovm_value::forward::{CanonicalArm, ThreadProjectionTable};
use neovm_value::value::Value;

fn main() {
    let mut table = ThreadProjectionTable::default();
    let _ = table.issue(CanonicalArm::Plain, Value::UNBOUND, false);
}
