//! Source-shared validation for the cold parallel-entry inventory.
//!
//! The source scanner and compile-contract fixtures include this exact file.
//! No runtime dispatch table stores these declarations.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Authority {
    #[allow(dead_code)] // E1 admits no Scalar handler; the typed catalog reserves it.
    Scalar,
    #[allow(dead_code)] // E1 admits no Sealed handler; E4/E5 must prove it.
    Sealed,
    World,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogKind {
    Primitive,
    Opcode,
    Leaf,
    Alias,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Classification {
    #[allow(dead_code)] // Scanner/negative-fixture input; never admitted to dispatch.
    Unclassified,
    Classified(Authority),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaEntry<'name> {
    pub kind: CatalogKind,
    pub name: &'name str,
    pub classification: Classification,
}

impl<'name> SchemaEntry<'name> {
    #[allow(dead_code)] // Source-shared compile-contract constructor.
    pub const fn primitive(name: &'name str, classification: Classification) -> Self {
        Self {
            kind: CatalogKind::Primitive,
            name,
            classification,
        }
    }

    #[allow(dead_code)] // Used by runtime const catalogs; scanner uses parsed records.
    pub const fn classified(kind: CatalogKind, name: &'name str, authority: Authority) -> Self {
        Self {
            kind,
            name,
            classification: Classification::Classified(authority),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaError {
    UnclassifiedPrimitive,
    DuplicateRegistration,
}

const fn same_name(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn same_kind(left: CatalogKind, right: CatalogKind) -> bool {
    match (left, right) {
        (CatalogKind::Primitive, CatalogKind::Primitive)
        | (CatalogKind::Opcode, CatalogKind::Opcode)
        | (CatalogKind::Leaf, CatalogKind::Leaf)
        | (CatalogKind::Alias, CatalogKind::Alias) => true,
        (CatalogKind::Primitive, CatalogKind::Opcode | CatalogKind::Leaf | CatalogKind::Alias)
        | (CatalogKind::Opcode, CatalogKind::Primitive | CatalogKind::Leaf | CatalogKind::Alias)
        | (CatalogKind::Leaf, CatalogKind::Primitive | CatalogKind::Opcode | CatalogKind::Alias)
        | (CatalogKind::Alias, CatalogKind::Primitive | CatalogKind::Opcode | CatalogKind::Leaf) => {
            false
        }
    }
}

/// A declaration is classified exactly once within its registry namespace.
pub const fn validate(entries: &[SchemaEntry<'_>]) -> Result<(), SchemaError> {
    let mut current = 0;
    while current < entries.len() {
        match entries[current].classification {
            Classification::Unclassified => return Err(SchemaError::UnclassifiedPrimitive),
            Classification::Classified(
                Authority::Scalar | Authority::Sealed | Authority::World,
            ) => {}
        }
        let mut earlier = 0;
        while earlier < current {
            if same_kind(entries[current].kind, entries[earlier].kind)
                && same_name(entries[current].name, entries[earlier].name)
            {
                return Err(SchemaError::DuplicateRegistration);
            }
            earlier += 1;
        }
        current += 1;
    }
    Ok(())
}

/// Const failure messages are also the compile-contract fixture diagnostics.
#[allow(dead_code)] // Runtime const catalogs/fixtures; scanner uses validate directly.
pub const fn assert_valid(entries: &[SchemaEntry<'_>]) {
    match validate(entries) {
        Ok(()) => {}
        Err(SchemaError::UnclassifiedPrimitive) => {
            panic!("primitive inventory: missing authority classification")
        }
        Err(SchemaError::DuplicateRegistration) => {
            panic!("primitive inventory: duplicate registration")
        }
    }
}
