//! P8/E4-T: the typed P3 parallel classes of the E1 cold catalog.
//!
//! DORMANT: there is no executor and no runtime consumer. The source
//! scanner validates these declarations against the scanned registrations and
//! emits the dense cold admission table into `effect_inventory.inc`; the
//! architecture gate and catalog tests read them. Aliases are absent by
//! construction: they inherit the resolved target's class exactly as in P3.

use super::effect_schema::{
    ArithOperands, CatalogKind, ParallelClass, ParallelDeclaration, ParallelSchemaError,
    ParallelVariant, PlainVarAccess, ReturnDisposition, SealedAuthority, validate_parallel,
};

const CONS_FIELD_READ: &str = "P3 review: single-object single-word cons-field read; no traversal (classification Car/Cdr/car/cdr).";
const CONS_FIELD_WRITE: &str =
    "P3 review: single-field write with the generational barrier (Setcar/Setcdr/setcar/setcdr).";
const CONS_ALLOCATION: &str =
    "P3 review: fresh unpublished cons cell/chain allocation (Cons/List/cons/list).";
const HASH_READ: &str = "P3 review: sealed-hash authority read (proof 2, Gethash).";
const CANONICAL_SYMBOL: &str =
    "P3 review: canonical C3 immutable symbol-name read (Keywordp/SymbolName).";
const HEADER_WORD: &str =
    "P3 review: one immutable header/type-tag word (or immutable header fields).";
const SCALAR_TAG: &str = "P3 review: immediate/tag-only word test; no heap dereference.";
const SCALAR_FRAME: &str = "P3 review: frame-local/owner-local stack/pc/constant-pool operation.";
const SCALAR_CONST: &str = "P3 review: compile-time constant; no state is read or written.";
const SCALAR_IDENTITY: &str =
    "P3 review: returns the argument word unchanged; no state is read or written.";
const RETURN_SPLIT: &str = "P3 review addendum Q6: Return is not purely frame-local; the NeedsSlowCleanup arm unwinds specpdl/calls and exits the debugger (bytecode/vm.rs:3952-3975). Owner-local mirrors GNU Breturn (bytecode.c:892-903).";
const PLAIN_VAR_SPLIT: &str = "E4 rev2 required change 1: owner-local through EffectivePlainKey admission; localized/forwarded/trapped symbols escape.";
const ARITH_SPLIT: &str = "E4 rev2 R6: numeric (fixnum/float/bignum) operands run owner-local; a marker operand escapes because a marker position is a buffer read.";

/// The checked-in canonical classification: 38 Sealed declarations (the
/// other 3 canonical Sealed names are aliases that inherit), 33 Scalar
/// declarations (1 canonical Scalar alias inherits), and 37 typed variants.
#[allow(dead_code)] // Cold E1 catalog; the scanner/tests read it.
pub(crate) const ADMITTED: &[ParallelDeclaration] = &[
    // Cons-field reads: one field of one cons cell (8).
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Car",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "CarSafe",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Cdr",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "CdrSafe",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "car",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "car-safe",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "cdr",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "cdr-safe",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldRead),
        variant: None,
        evidence: CONS_FIELD_READ,
    },
    // Cons-field writes: one field with the generational barrier (4).
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Setcar",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldWrite),
        variant: None,
        evidence: CONS_FIELD_WRITE,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Setcdr",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldWrite),
        variant: None,
        evidence: CONS_FIELD_WRITE,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "setcar",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldWrite),
        variant: None,
        evidence: CONS_FIELD_WRITE,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "setcdr",
        class: ParallelClass::Sealed(SealedAuthority::ConsFieldWrite),
        variant: None,
        evidence: CONS_FIELD_WRITE,
    },
    // Fresh cons allocation: unpublished until complete (4).
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Cons",
        class: ParallelClass::Sealed(SealedAuthority::ConsAllocation),
        variant: None,
        evidence: CONS_ALLOCATION,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "List",
        class: ParallelClass::Sealed(SealedAuthority::ConsAllocation),
        variant: None,
        evidence: CONS_ALLOCATION,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "cons",
        class: ParallelClass::Sealed(SealedAuthority::ConsAllocation),
        variant: None,
        evidence: CONS_ALLOCATION,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "list",
        class: ParallelClass::Sealed(SealedAuthority::ConsAllocation),
        variant: None,
        evidence: CONS_ALLOCATION,
    },
    // Sealed hash read (1).
    ParallelDeclaration {
        kind: CatalogKind::Leaf,
        name: "Gethash",
        class: ParallelClass::Sealed(SealedAuthority::HashRead),
        variant: None,
        evidence: HASH_READ,
    },
    // Canonical C3 symbol-name read (2).
    ParallelDeclaration {
        kind: CatalogKind::Leaf,
        name: "Keywordp",
        class: ParallelClass::Sealed(SealedAuthority::CanonicalSymbol),
        variant: None,
        evidence: CANONICAL_SYMBOL,
    },
    ParallelDeclaration {
        kind: CatalogKind::Leaf,
        name: "SymbolName",
        class: ParallelClass::Sealed(SealedAuthority::CanonicalSymbol),
        variant: None,
        evidence: CANONICAL_SYMBOL,
    },
    // Immutable header/type-tag reads (19).
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "arrayp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "bool-vector-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "bufferp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "byte-code-function-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "char-table-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "closurep",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "hash-table-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "integerp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "interpreted-function-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "markerp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "module-function-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "multibyte-string-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "numberp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "sequencep",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "subrp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "symbol-with-pos-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "user-ptrp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "vector-or-char-table-p",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "vectorp",
        class: ParallelClass::Sealed(SealedAuthority::HeaderWord),
        variant: None,
        evidence: HEADER_WORD,
    },
    // Scalar frame/stack/tag operations (19 opcodes).
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Consp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Constant",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "DiscardN",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Dup",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Goto",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "GotoIfNil",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "GotoIfNilElsePop",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "GotoIfNotNil",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "GotoIfNotNilElsePop",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Listp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Nil",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Not",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Null",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Pop",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "StackRef",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "StackSet",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Stringp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "TrapOutOfRangeConstant",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "True",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_FRAME,
    },
    // Scalar tag/constant/identity primitives (14).
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "atom",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "byteorder",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_CONST,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "characterp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "consp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "floatp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "identity",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_IDENTITY,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "libxml-available-p",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_CONST,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "listp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "native-comp-available-p",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_CONST,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "nlistp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "null",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "stringp",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_TAG,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "treesit-available-p",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_CONST,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "zlib-available-p",
        class: ParallelClass::Scalar,
        variant: None,
        evidence: SCALAR_CONST,
    },
    // E4 R1 typed split: Return (canonical P3 class stays World).
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Return",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Return(ReturnDisposition::OwnerLocal)),
        evidence: RETURN_SPLIT,
    },
    // E4 R1: Plain variable opcodes, keyed by EffectivePlainKey admission.
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "VarRef",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::PlainVar(PlainVarAccess::Ref)),
        evidence: PLAIN_VAR_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "VarSet",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::PlainVar(PlainVarAccess::Set)),
        evidence: PLAIN_VAR_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "VarBind",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::PlainVar(PlainVarAccess::Bind)),
        evidence: PLAIN_VAR_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Unbind",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::PlainVar(PlainVarAccess::Unbind)),
        evidence: PLAIN_VAR_SPLIT,
    },
    // E4 R6: numeric arithmetic opcodes (peer names Plus/Minus/Mult/Quo = Add/Sub/Mul/Div).
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Add1",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Sub1",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Add",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Sub",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Mul",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Div",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Rem",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Max",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Min",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Negate",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Lss",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Gtr",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Leq",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Geq",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Opcode,
        name: "Eqlsign",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    // E4 R6: numeric arithmetic primitives.
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "+",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "-",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "*",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "/",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "%",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "mod",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "1+",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "1-",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "<",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: ">",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "<=",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: ">=",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "=",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "/=",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "max",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "min",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
    ParallelDeclaration {
        kind: CatalogKind::Primitive,
        name: "abs",
        class: ParallelClass::World,
        variant: Some(ParallelVariant::Arith(ArithOperands::Numeric)),
        evidence: ARITH_SPLIT,
    },
];

/// The E4 R1 Return designation. The executor chooses NeedsSerialCleanup at
/// runtime from the frame flags; the catalog admits the typed split.
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const RETURN_VARIANT_NAMES: &[(&str, ReturnDisposition)] =
    &[("Return", ReturnDisposition::OwnerLocal)];

/// The E4 R1 Plain variable opcodes and their effective access kind.
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const PLAIN_VAR_VARIANT_NAMES: &[(&str, PlainVarAccess)] = &[
    ("VarRef", PlainVarAccess::Ref),
    ("VarSet", PlainVarAccess::Set),
    ("VarBind", PlainVarAccess::Bind),
    ("Unbind", PlainVarAccess::Unbind),
];

/// The E4 R6 numeric arithmetic set (opcodes and primitives).
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const ARITH_VARIANT_NAMES: &[&str] = &[
    "Add1", "Sub1", "Add", "Sub", "Mul", "Div", "Rem", "Max", "Min", "Negate", "Lss", "Gtr", "Leq",
    "Geq", "Eqlsign", "+", "-", "*", "/", "%", "mod", "1+", "1-", "<", ">", "<=", ">=", "=", "/=",
    "max", "min", "abs",
];

/// Registrations added to E1 after the canonical P3 survey. They are not
/// P3 names; the World default covers them, and the architecture gate
/// excludes them from the canonical 1,650/41/34 name counts.
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const POST_P3_WORLD_REGISTRATIONS: &[&str] =
    &["neomacs-set-extra-spacing", "neomacs-set-scroll-indicators"];

/// Canonical P3 totals over the 1,725 classified names.
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const CANONICAL_WORLD_NAMES: usize = 1_650;
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const CANONICAL_SEALED_NAMES: usize = 41;
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const CANONICAL_SCALAR_NAMES: usize = 34;

/// Exact generated-table expectations (all inventory entries and kinds).
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_INVENTORY_ENTRIES: usize = 1_739;
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_PRIMITIVE_ENTRIES: usize = 1_578;
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_OPCODE_ENTRIES: usize = 107;
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_LEAF_ENTRIES: usize = 23;
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_ALIAS_ENTRIES: usize = 31;

/// Declared-class counts over the generated table: 38 declared Sealed + the
/// 3 Sealed aliases, 33 declared Scalar + the 1 Scalar alias, and the rest
/// World (the 12 dual leaf/opcode entries repeat a classified name).
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_SEALED_ENTRIES: usize = 41;
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_SCALAR_ENTRIES: usize = 34;
#[allow(dead_code)] // Read by the architecture gate.
pub(crate) const EXPECTED_WORLD_ENTRIES: usize = 1_664;

/// Exact declaration and canonical-name expectations.
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const EXPECTED_CANONICAL_NAMES: usize = 1_725;
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const EXPECTED_SEALED_DECLARATIONS: usize = 38;
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const EXPECTED_SCALAR_DECLARATIONS: usize = 33;
#[allow(dead_code)] // Read by the architecture gate and catalog tests.
pub(crate) const EXPECTED_VARIANT_DECLARATIONS: usize = 37;

/// One resolved catalog row as the closed-world gate sees it. The source
/// scanner builds rows over scanned registrations; the catalog tests build
/// them over the checked-in generated table. Both run the same gate.
#[allow(dead_code)] // Cold gate input; the scanner and catalog tests build rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogRow<'name> {
    pub kind: CatalogKind,
    pub name: &'name str,
    pub class: ParallelClass,
    pub variant: Option<ParallelVariant>,
    pub alias_target: Option<&'name str>,
}

#[allow(dead_code)] // Cold gate diagnostics for the scanner and catalog tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParallelCatalogError<'name> {
    Shape(ParallelSchemaError),
    MissingDeclarationTarget {
        kind: CatalogKind,
        name: &'name str,
    },
    DeclarationMismatch {
        kind: CatalogKind,
        name: &'name str,
    },
    UndeclaredClass {
        kind: CatalogKind,
        name: &'name str,
    },
    AliasInheritance {
        name: &'name str,
    },
    VariantMismatch {
        name: &'name str,
    },
    DuplicateCanonicalName {
        name: &'name str,
    },
    CountMismatch {
        what: &'static str,
        expected: usize,
        found: usize,
    },
}

impl std::fmt::Display for ParallelCatalogError<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape(error) => write!(formatter, "shape: {error:?}"),
            Self::MissingDeclarationTarget { kind, name } => {
                write!(
                    formatter,
                    "declaration for {kind:?} {name:?} has no registration"
                )
            }
            Self::DeclarationMismatch { kind, name } => write!(
                formatter,
                "generated class for {kind:?} {name:?} disagrees with its declaration"
            ),
            Self::UndeclaredClass { kind, name } => write!(
                formatter,
                "non-World class for {kind:?} {name:?} has no evidence-bearing declaration"
            ),
            Self::AliasInheritance { name } => write!(
                formatter,
                "alias {name:?} does not inherit its resolved target's class"
            ),
            Self::VariantMismatch { name } => {
                write!(
                    formatter,
                    "typed variant for {name:?} is not the designated one"
                )
            }
            Self::DuplicateCanonicalName { name } => write!(
                formatter,
                "canonical name {name:?} carries divergent classes across entries"
            ),
            Self::CountMismatch {
                what,
                expected,
                found,
            } => {
                write!(formatter, "{what}: expected {expected}, found {found}")
            }
        }
    }
}

#[allow(dead_code)] // Used by the cold gate below.
fn designated_variant(name: &str) -> Option<ParallelVariant> {
    if let Some((_, disposition)) = RETURN_VARIANT_NAMES
        .iter()
        .find(|(entry, _)| *entry == name)
    {
        return Some(ParallelVariant::Return(*disposition));
    }
    if let Some((_, access)) = PLAIN_VAR_VARIANT_NAMES
        .iter()
        .find(|(entry, _)| *entry == name)
    {
        return Some(ParallelVariant::PlainVar(*access));
    }
    ARITH_VARIANT_NAMES
        .iter()
        .any(|entry| *entry == name)
        .then_some(ParallelVariant::Arith(ArithOperands::Numeric))
}

#[allow(dead_code)] // Used by the cold gate below.
fn count_mismatch<'name>(
    what: &'static str,
    expected: usize,
    found: usize,
) -> Result<(), ParallelCatalogError<'name>> {
    if expected == found {
        Ok(())
    } else {
        Err(ParallelCatalogError::CountMismatch {
            what,
            expected,
            found,
        })
    }
}

/// The closed-world architecture gate. It enforces both directions between
/// the evidence-bearing declarations and one resolved catalog snapshot:
/// every declaration names a live registration and the snapshot carries
/// exactly its class and typed variant; every non-default class carries an
/// evidence-bearing declaration; aliases inherit their resolved primitive
/// target; and the canonical P3 totals and typed variant designations hold
/// exactly. A new `world_subr!` registration therefore stays World until a
/// reviewed declaration (or a `POST_P3_WORLD_REGISTRATIONS` entry) says
/// otherwise, and flipping one Sealed entry to Scalar fails the gate.
#[allow(dead_code)] // Cold architecture gate; tests and the source scanner call it.
pub(crate) fn canonical_parallel_error<'name>(
    rows: &[CatalogRow<'name>],
    declarations: &[ParallelDeclaration],
) -> Result<(), ParallelCatalogError<'name>> {
    if let Err(error) = validate_parallel(declarations) {
        return Err(ParallelCatalogError::Shape(error));
    }

    for declaration in declarations {
        let row = rows
            .iter()
            .find(|row| row.kind == declaration.kind && row.name == declaration.name)
            .ok_or(ParallelCatalogError::MissingDeclarationTarget {
                kind: declaration.kind,
                name: declaration.name,
            })?;
        if row.class != declaration.class || row.variant != declaration.variant {
            return Err(ParallelCatalogError::DeclarationMismatch {
                kind: declaration.kind,
                name: declaration.name,
            });
        }
    }

    for row in rows {
        if row.kind == CatalogKind::Alias {
            let inherited = row
                .alias_target
                .and_then(|target| {
                    rows.iter().find(|candidate| {
                        candidate.kind == CatalogKind::Primitive && candidate.name == target
                    })
                })
                .map(|target| (target.class, target.variant))
                .unwrap_or((ParallelClass::World, None));
            if (row.class, row.variant) != inherited {
                return Err(ParallelCatalogError::AliasInheritance { name: row.name });
            }
            continue;
        }
        if row.class == ParallelClass::World && row.variant.is_none() {
            continue;
        }
        if !declarations
            .iter()
            .any(|declaration| declaration.kind == row.kind && declaration.name == row.name)
        {
            return Err(ParallelCatalogError::UndeclaredClass {
                kind: row.kind,
                name: row.name,
            });
        }
    }

    for row in rows {
        if row.kind == CatalogKind::Alias {
            continue;
        }
        let admitted = match (designated_variant(row.name), row.variant) {
            (Some(expected), Some(actual)) => {
                expected == actual && row.class == ParallelClass::World
            }
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        };
        if !admitted {
            return Err(ParallelCatalogError::VariantMismatch { name: row.name });
        }
    }

    let mut canonical: Vec<(&str, ParallelClass, Option<ParallelVariant>)> = Vec::new();
    for row in rows {
        if POST_P3_WORLD_REGISTRATIONS.contains(&row.name) {
            continue;
        }
        match canonical.iter().find(|(name, _, _)| *name == row.name) {
            Some((_, class, variant)) if *class == row.class && *variant == row.variant => {}
            Some(_) => {
                return Err(ParallelCatalogError::DuplicateCanonicalName { name: row.name });
            }
            None => canonical.push((row.name, row.class, row.variant)),
        }
    }
    let mut world_names = 0;
    let mut sealed_names = 0;
    let mut scalar_names = 0;
    for (_, class, _) in &canonical {
        match class {
            ParallelClass::World => world_names += 1,
            ParallelClass::Sealed(_) => sealed_names += 1,
            ParallelClass::Scalar => scalar_names += 1,
        }
    }
    count_mismatch("canonical names", EXPECTED_CANONICAL_NAMES, canonical.len())?;
    count_mismatch("canonical World names", CANONICAL_WORLD_NAMES, world_names)?;
    count_mismatch(
        "canonical Sealed names",
        CANONICAL_SEALED_NAMES,
        sealed_names,
    )?;
    count_mismatch(
        "canonical Scalar names",
        CANONICAL_SCALAR_NAMES,
        scalar_names,
    )?;

    let mut world_entries = 0;
    let mut sealed_entries = 0;
    let mut scalar_entries = 0;
    let mut primitives = 0;
    let mut opcodes = 0;
    let mut leaves = 0;
    let mut aliases = 0;
    for row in rows {
        match row.class {
            ParallelClass::World => world_entries += 1,
            ParallelClass::Sealed(_) => sealed_entries += 1,
            ParallelClass::Scalar => scalar_entries += 1,
        }
        match row.kind {
            CatalogKind::Primitive => primitives += 1,
            CatalogKind::Opcode => opcodes += 1,
            CatalogKind::Leaf => leaves += 1,
            CatalogKind::Alias => aliases += 1,
        }
    }
    count_mismatch("inventory entries", EXPECTED_INVENTORY_ENTRIES, rows.len())?;
    count_mismatch("World entries", EXPECTED_WORLD_ENTRIES, world_entries)?;
    count_mismatch("Sealed entries", EXPECTED_SEALED_ENTRIES, sealed_entries)?;
    count_mismatch("Scalar entries", EXPECTED_SCALAR_ENTRIES, scalar_entries)?;
    count_mismatch("Primitive entries", EXPECTED_PRIMITIVE_ENTRIES, primitives)?;
    count_mismatch("Opcode entries", EXPECTED_OPCODE_ENTRIES, opcodes)?;
    count_mismatch("Leaf entries", EXPECTED_LEAF_ENTRIES, leaves)?;
    count_mismatch("Alias entries", EXPECTED_ALIAS_ENTRIES, aliases)?;

    let mut declared_sealed = 0;
    let mut declared_scalar = 0;
    let mut declared_variants = 0;
    for declaration in declarations {
        match declaration.class {
            ParallelClass::World => {}
            ParallelClass::Sealed(_) => declared_sealed += 1,
            ParallelClass::Scalar => declared_scalar += 1,
        }
        if declaration.variant.is_some() {
            declared_variants += 1;
        }
    }
    count_mismatch(
        "Sealed declarations",
        EXPECTED_SEALED_DECLARATIONS,
        declared_sealed,
    )?;
    count_mismatch(
        "Scalar declarations",
        EXPECTED_SCALAR_DECLARATIONS,
        declared_scalar,
    )?;
    count_mismatch(
        "typed variant declarations",
        EXPECTED_VARIANT_DECLARATIONS,
        declared_variants,
    )?;
    Ok(())
}
