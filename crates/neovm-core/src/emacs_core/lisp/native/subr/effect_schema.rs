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

// ---------------------------------------------------------------------------
// P8/E4-T: typed parallel classes for the cold E1 catalog.
//
// This vocabulary is shared with the source scanner exactly like the
// registration grammar above. Nothing here is read by a serial dispatcher;
// the generated dense admission table is cold const data for E4's executor and
// the architecture gate.
// ---------------------------------------------------------------------------

/// The single-object authority a `Sealed` entry executes under. The payload is
/// typed so a Sealed entry states *which* authority E1 grants it; there is no
/// boolean "is sealed" flag and no string-keyed authority vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SealedAuthority {
    /// One field of one cons cell (read).
    ConsFieldRead,
    /// One field of one cons cell (write, with the generational barrier).
    ConsFieldWrite,
    /// One fresh unpublished cons cell or cons chain.
    ConsAllocation,
    /// The sealed hash-table read authority (proof 2).
    HashRead,
    /// The canonical C3 immutable symbol-name read.
    CanonicalSymbol,
    /// One immutable header/type-tag word (or immutable header fields).
    HeaderWord,
}

/// The two dispositions of the Return opcode (E4 peer required change 1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReturnDisposition {
    OwnerLocal,
    #[allow(dead_code)]
    // E4's executor selects this arm at runtime; the cold catalog admits the split.
    NeedsSerialCleanup,
}

/// The Plain variable opcode access admitted through an `EffectivePlainKey`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlainVarAccess {
    Ref,
    Set,
    Bind,
    Unbind,
}

/// The operand split of the R6 numeric arithmetic set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArithOperands {
    Numeric,
    #[allow(dead_code)] // R6: a marker operand escapes; the executor selects this arm at runtime.
    MarkerInvolved,
}

/// The P3 canonical parallel class of one E1 entry. World is the default for
/// every registration that carries no explicit evidence-bearing declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelClass {
    World,
    Sealed(SealedAuthority),
    Scalar,
}

/// The typed per-entry variant refining a World-class P3 entry (E4 R1/R6).
/// The canonical P3 verdict stays World; the variant names the executor split.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelVariant {
    Return(ReturnDisposition),
    PlainVar(PlainVarAccess),
    Arith(ArithOperands),
}

/// One evidence-bearing parallel declaration in the single-source catalog.
/// `kind`/`name` must match a scanned registration. Aliases never appear here:
/// they inherit their resolved target's class, exactly as in P3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelDeclaration {
    pub kind: CatalogKind,
    pub name: &'static str,
    pub class: ParallelClass,
    pub variant: Option<ParallelVariant>,
    pub evidence: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelSchemaError {
    /// Every claim carries a source-review evidence note.
    MissingEvidence,
    /// A World-class declaration must carry a typed variant; otherwise it
    /// claims nothing and belongs in the default set.
    EmptyDeclaration,
    /// Aliases inherit; declaring one would freeze its target.
    AliasDeclaration,
    /// A typed variant refines only a canonical World entry. Giving one to a
    /// Sealed/Scalar entry would silently reclassify the P3 verdict.
    VariantClassMismatch,
    DuplicateDeclaration,
}

/// Shape validation for the typed declaration list. Closed-world checks
/// (names exist, designations, exact counts) belong to the architecture gate,
/// which has the scanned registrations; this const check is shared with the
/// source scanner and needs no catalog context.
pub const fn validate_parallel(
    declarations: &[ParallelDeclaration],
) -> Result<(), ParallelSchemaError> {
    let mut current = 0;
    while current < declarations.len() {
        let declaration = &declarations[current];
        if declaration.evidence.is_empty() {
            return Err(ParallelSchemaError::MissingEvidence);
        }
        if matches!(declaration.kind, CatalogKind::Alias) {
            return Err(ParallelSchemaError::AliasDeclaration);
        }
        if matches!(declaration.class, ParallelClass::World) && declaration.variant.is_none() {
            return Err(ParallelSchemaError::EmptyDeclaration);
        }
        if !matches!(declaration.class, ParallelClass::World) && declaration.variant.is_some() {
            return Err(ParallelSchemaError::VariantClassMismatch);
        }
        let mut earlier = 0;
        while earlier < current {
            if same_kind(declaration.kind, declarations[earlier].kind)
                && same_name(declaration.name, declarations[earlier].name)
            {
                return Err(ParallelSchemaError::DuplicateDeclaration);
            }
            earlier += 1;
        }
        current += 1;
    }
    Ok(())
}

/// Const failure messages are also the parallel architecture-gate diagnostics.
#[allow(dead_code)] // Runtime const catalogs/fixtures; the scanner calls validate_parallel.
pub const fn assert_parallel_valid(declarations: &[ParallelDeclaration]) {
    match validate_parallel(declarations) {
        Ok(()) => {}
        Err(ParallelSchemaError::MissingEvidence) => {
            panic!("parallel catalog: missing evidence note")
        }
        Err(ParallelSchemaError::EmptyDeclaration) => {
            panic!("parallel catalog: declaration claims no class or variant")
        }
        Err(ParallelSchemaError::AliasDeclaration) => {
            panic!("parallel catalog: aliases inherit their target")
        }
        Err(ParallelSchemaError::VariantClassMismatch) => {
            panic!("parallel catalog: typed variant on a non-World canonical class")
        }
        Err(ParallelSchemaError::DuplicateDeclaration) => {
            panic!("parallel catalog: duplicate declaration")
        }
    }
}
