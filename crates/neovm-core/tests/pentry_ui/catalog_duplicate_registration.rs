#[path = "../../src/emacs_core/lisp/native/subr/effect_schema.rs"]
mod effect_schema;

use effect_schema::{Authority, CatalogKind, SchemaEntry, assert_valid};

const _: () = assert_valid(&[
    SchemaEntry::classified(CatalogKind::Primitive, "duplicate", Authority::World),
    SchemaEntry::classified(CatalogKind::Primitive, "duplicate", Authority::Sealed),
]);

fn main() {}
