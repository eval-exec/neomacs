#[path = "../../src/emacs_core/lisp/native/subr/effect_schema.rs"]
mod effect_schema;

use effect_schema::{Authority, Classification, SchemaEntry, assert_valid};

const CLASSIFICATION: Classification = Classification::Classified(Authority::World);
const _: () = assert_valid(&[
    SchemaEntry::primitive("deliberately-unclassified", CLASSIFICATION),
]);

fn main() {
    let _ = Authority::World;
}
