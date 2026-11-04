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

// ---------------------------------------------------------------------------
// E4-S0: the executor-facing dispatch vocabulary.
//
// The generated cold inventory carries one `DispatchSlot` per E1 entry: the
// Scalar/Sealed/World arm E4 dispatches on plus the green-guard fast-path
// re-entry axis. Nothing here is read by a serial dispatcher; the source
// scanner shares these types and the projection function so the generated
// table, the closed-world gate and the E4 accessors cannot drift.
// ---------------------------------------------------------------------------

/// Whether an inlined call site's fast path can run Lisp or release the
/// Baton (the green-guard re-entry axis, folded into the E1 catalog).
///
/// Orthogonal to [`ParallelClass`], which classifies parallel admission, not
/// fast-path re-entrancy: `mod` is `World` for admission and `CannotReenter`
/// for the JIT's baked arithmetic intrinsic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FastPathReentry {
    /// Machine arithmetic over values already in registers: cannot call
    /// Lisp, grow eval depth, allocate or serve attention at the site.
    /// Signaling (`arith-error`) exits compiled code through the slow path.
    CannotReenter,
    /// The fast path can run Lisp or release the Baton (the conservative
    /// default for every catalog member and every unknown name).
    MayReenter,
}

/// The native-arithmetic intrinsics the JIT bakes as machine ops
/// (`arith_op_inlines`). This is the ONE declaration of the call-free set:
/// the source scanner folds it into the generated dispatch table, the
/// lowering asks the generated table, and the pin test compares the JIT's
/// own inlining set against it. Ascending name order; the generated lookup
/// binary-searches its sorted projection.
pub const INLINED_NATIVE_OPS: [&str; 5] = ["logand", "logior", "lognot", "logxor", "mod"];

/// The executor arm of one admitted entry (E4-DESIGN §2/§3.1).
///
/// A `World` arm is not necessarily "always serial": the [`WorldSplit`]
/// names the typed R1/R6 refinement whose runtime guard may still select the
/// World escape. A table miss is therefore exactly `World(WorldSplit::Escape)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelArm {
    /// Runs inline in the executor: no authority, no heap payload access.
    Scalar,
    /// Runs through the typed single-object sealed authority.
    Sealed(SealedAuthority),
    /// Not executable owner-locally as written; the split names the typed
    /// refinement, and `Escape` is the conservative default.
    World(WorldSplit),
}

/// The typed disposition of a World-class entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldSplit {
    /// Always takes the serial escape (every unlisted registration).
    Escape,
    /// `Op::Return`: owner-local unwind, or serial cleanup when the frame
    /// needs Lisp at exit (E4 peer required change 1).
    Return(ReturnDisposition),
    /// `VarRef`/`VarSet`/`VarBind`/`Unbind` through an `EffectivePlainKey`.
    PlainVar(PlainVarAccess),
    /// The R6 numeric-arithmetic split: numeric operands are owner-local,
    /// a marker operand escapes.
    Arith(ArithOperands),
}

/// One generated dispatch row: the arm and the folded re-entry axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchSlot {
    pub arm: ParallelArm,
    pub reentry: FastPathReentry,
}

/// A class/variant combination no entry may carry: a typed split refines a
/// canonical World entry only (the same rule `validate_parallel` enforces on
/// declarations; the gate applies it to resolved rows too).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchShapeError {
    VariantOnNonWorldClass,
}

/// The one projection from a resolved `(class, variant)` pair to an executor
/// arm. The generated table, the closed-world gate and the E4 accessors share
/// this function; an impossible combination is a typed error, never a
/// silently chosen arm.
pub const fn dispatch_arm(
    class: ParallelClass,
    variant: Option<ParallelVariant>,
) -> Result<ParallelArm, DispatchShapeError> {
    match (class, variant) {
        (ParallelClass::World, None) => Ok(ParallelArm::World(WorldSplit::Escape)),
        (ParallelClass::World, Some(ParallelVariant::Return(disposition))) => {
            Ok(ParallelArm::World(WorldSplit::Return(disposition)))
        }
        (ParallelClass::World, Some(ParallelVariant::PlainVar(access))) => {
            Ok(ParallelArm::World(WorldSplit::PlainVar(access)))
        }
        (ParallelClass::World, Some(ParallelVariant::Arith(operands))) => {
            Ok(ParallelArm::World(WorldSplit::Arith(operands)))
        }
        (ParallelClass::Sealed(authority), None) => Ok(ParallelArm::Sealed(authority)),
        (ParallelClass::Scalar, None) => Ok(ParallelArm::Scalar),
        (ParallelClass::Sealed(_) | ParallelClass::Scalar, Some(_)) => {
            Err(DispatchShapeError::VariantOnNonWorldClass)
        }
    }
}
