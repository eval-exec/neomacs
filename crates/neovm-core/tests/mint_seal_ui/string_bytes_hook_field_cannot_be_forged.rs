//! N9/PC2: the byte-mutation stamp table itself is never nameable. The
//! installer is the only authority (idempotent same-table, typed refusal for
//! a different table); reaching past it to forge or clear the table must
//! fail. Private at its defining path in neovm-value (E0603): PC2 promotes
//! the installer, never the field.
fn main() {
    let _ = &neovm_value::heap_types::NOTE_STRING_BYTES_CHANGED;
}
