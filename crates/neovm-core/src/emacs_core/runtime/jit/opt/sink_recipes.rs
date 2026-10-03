//! Func-owned numeric and Cons identities with explicit physical SSA fields.
//! Independent validation is separate from transformation planning.
//!
//! Threading: IDs/scalars are owned by one compilation and immutable at worker
//! transfer. Runtime boxes are represented by real Tagged SSA values, never
//! retained in Rust/TLS. No new runtime/Context/leaf ABI layout is proposed.

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::opt::{
    ir::{Block, FrameId, Func, Inst, Value},
    types::TypeSet,
};
use std::collections::HashMap;

/// Static compiler handle; never a dynamic loop-instance identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct RecipeId(pub(crate) u32);

/// Immutable version of explicit SSA cache/field values at an exact point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct RecipeVersionId(pub(crate) u32);

/// A real instruction cut. SourcePre/Post additionally use the same validated
/// source-pc cut calculation as ordinary source-state dominance verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RecipePoint {
    Entry(Block),
    Before(Inst),
    After(Inst),
    Term(Block),
    SourcePre(u32),
    SourcePost(u32),
}

/// Exactly four physical SSA fields. RawWord is an opaque I64 rep:
/// before readiness it contains original Lisp bits; afterwards a Float marker
/// or the exact tagged fixnum. It NEVER becomes a root or ordinary Lisp word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NumericFields {
    pub(crate) payload: Value,  // RawF64; dummy exact +0 until ready
    pub(crate) word: Value,     // RawWord; paired with ready and real_box
    pub(crate) ready: Value,    // Bool; no payload dereference/use while false
    pub(crate) real_box: Value, // Tagged: exact borrowed seed, NIL, Float, or fix
}

/// Cons fields name real SSA semantic values, including other logical recipe
/// owners when nested. The physical box cache is ALSO a real Tagged SSA value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ConsFields {
    pub(crate) car: Value,
    pub(crate) cdr: Value,
    pub(crate) real_box: Value, // Tagged CONS|NIL; initially exact NIL
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RecipeFields {
    Number(NumericFields),
    Cons(ConsFields),
}

/// Borrowable admits TOP only through verified Borrow/phi provenance. Strict
/// modes can omit unsupported source cases but may never eager-unbox a seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumericMode {
    FloatOnly,
    FixOrFloat,
    Borrowable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecipeKind {
    Number(NumericMode),
    Cons,
}

/// The precise producer certifies field meaning; declarations alone do not.
/// All field projections of an operation are contiguous and complete before
/// its logical result is used by an observer or crosses an edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RecipeOrigin {
    Borrow {
        inst: Inst,
        original: Value,
    },
    NumericSource {
        inst: Inst,
        original_op: Op,
        frame: FrameId,
        pc: u32,
    },
    ConsSource {
        inst: Inst,
        original_op: Op,
        frame: FrameId,
        pc: u32,
    },
    Phi {
        block: Block,
        field_params: Box<[Value]>,
    },
    SameIdentityView {
        inst: Inst,
        input: Value,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct OwnerRecipe {
    pub(crate) owner: Value, // exact original logical source/result/phi ID
    pub(crate) kind: RecipeKind,
    pub(crate) semantic_type: TypeSet,
    pub(crate) origin: RecipeOrigin,
    pub(crate) definition_version: RecipeVersionId,
}

/// CacheAfter points to an actual materializer + actual SSA box projection.
/// It is invalid to overwrite a definition version or consult a future cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VersionCause {
    Definition,
    Parameter,
    CacheAfter {
        previous: RecipeVersionId,
        materialize: Inst,
        box_projection: Inst,
    },
    SameIdentity {
        input: RecipeVersionId,
    },
    /// Proven dynamic identity alias with its own physical phi/view fields.
    /// Non-box fields stay those of previous; input supplies only the cache.
    AliasCacheAfter {
        previous: RecipeVersionId,
        input: RecipeVersionId,
    },
    /// One dominating logical owner, with different cache versions on arms.
    /// Payload/word/ready retain the exact owner's dominating SSA fields; only
    /// real_box is a new Tagged block param. Entries cover EVERY edge occurrence.
    CachePhi {
        block: Block,
        box_param: Value,
        incoming: Box<[CachePhiEdge]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CachePhiEdge {
    pub(crate) source: Block,
    pub(crate) edge_index: u32,
    pub(crate) version: RecipeVersionId,
}

#[derive(Clone, Debug)]
pub(crate) struct RecipeVersion {
    pub(crate) owner: Value,
    pub(crate) fields: RecipeFields,
    pub(crate) cause: VersionCause,
}

/// Captures one original frame at ONE point; repeated use of the same FrameId
/// before/after a box instruction MUST retain distinct versions.
#[derive(Clone, Debug, Default)]
pub(crate) struct FrameRecipeView {
    /// Sorted by resolved original logical owner. Ordinary Tagged stack slots
    /// remain in the unchanged FrameState, without an invented recipe entry.
    pub(crate) versions: Box<[(Value, RecipeVersionId)]>,
}

/// One edge's complete tuple for an original logical phi. Edge occurrence is
/// explicit: parallel true/false edges to one target are not interchangeable.
#[derive(Clone, Debug)]
pub(crate) struct RecipeEdge {
    pub(crate) source: Block,
    pub(crate) edge_index: u32,
    pub(crate) target: Block,
    pub(crate) owner_param: Value,
    pub(crate) incoming_owner: Value,
    pub(crate) incoming_version: RecipeVersionId,
    pub(crate) field_args: Box<[Value]>,
}

/// Sole Func-owned authoritative table. Fresh identity equivalence is proved
/// by source/edge lineage; equal fields or allocation-site IDs are insufficient.
#[derive(Clone, Debug, Default)]
pub(crate) struct SinkRecipes {
    pub(crate) owners: HashMap<Value, OwnerRecipe>,
    pub(crate) versions: Vec<RecipeVersion>,
    pub(crate) frames: HashMap<(RecipePoint, FrameId), FrameRecipeView>,
    pub(crate) uses: HashMap<(RecipePoint, Value), RecipeVersionId>,
    pub(crate) edges: Vec<RecipeEdge>,
    /// Front-owned scalar source PCs; no worker Lisp lookup or identity cache.
    /// Backend requires its separate immutable callee/builtin witness too.
    pub(crate) source_sqrt_sites: std::collections::HashSet<u32>,
}

/// Selected recipe instructions, independently validated before emission.
/// Their physical shapes and effects are checked separately from provenance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SinkOp {
    BorrowNum,
    SourceNum(Op),
    /// Original Call(1), selected only by front-captured valid scalar PCs.
    /// The backend separately checks the immutable callee/builtin witness.
    SourceSqrt,
    SourceCons(Op),
    RecipeField(RecipeField),
    MaterializeNum,
    MaterializeCons,
    CacheBoxAfter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RecipeField {
    Payload,
    Word,
    Ready,
    RealBox,
    Car,
    Cdr,
}

/// Bounded, compiler-owned diagnostics: never stringly runtime state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SinkVerifyReason {
    MissingOwner,
    WrongFieldRepresentation,
    UngroundedTuple,
    InvalidBorrow,
    InvalidSourceOperation,
    IncompleteTuple,
    WrongEdgeTuple,
    NonDominatingVersion,
    FutureBoxVersion,
    LostPartialAlias,
    CyclicCons,
    UnknownRawWordUse,
    WrongCachePhi,
    StaleBoxVersion,
    InvalidPoint,
    MissingRootView,
    AnalysisLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SinkVerifyError {
    pub(crate) owner: Value,
    pub(crate) point: Option<RecipePoint>,
    pub(crate) reason: SinkVerifyReason,
}

/// Produced only by complete provenance/SSA verification; not constructible by
/// a transformation or trusted from census. Actual implementation keeps fields
/// private and records logical owners, physical fields and supported sites.
pub(crate) struct VerifiedSinkRecipes<'a> {
    // Implementation keeps construction private to the independent verifier.
    pub(super) recipes: &'a SinkRecipes,
    /// Built independently from grounded same-edge lineage; never supplied by
    /// a transformation or numerical payload equality. Point presence matters.
    pub(super) canonical: HashMap<(RecipePoint, Value), Value>,
    /// Independent cause/edge proof, indexed by RecipeVersionId; never inferred
    /// merely from a Tagged declaration or a transform's planning predicate.
    pub(super) boxed: Vec<bool>,
}

impl<'a> VerifiedSinkRecipes<'a> {
    pub(crate) fn guaranteed_boxed(&self, id: RecipeVersionId) -> bool {
        self.boxed.get(id.0 as usize).copied().unwrap_or(false)
    }
    pub(crate) fn version_at(&self, point: RecipePoint, owner: Value) -> Option<RecipeVersionId> {
        self.recipes.uses.get(&(point, owner)).copied()
    }
    pub(crate) fn version(&self, id: RecipeVersionId) -> Option<&'a RecipeVersion> {
        self.recipes.versions.get(id.0 as usize)
    }
    pub(crate) fn frame_view(
        &self,
        point: RecipePoint,
        frame: FrameId,
    ) -> Option<&'a FrameRecipeView> {
        self.recipes.frames.get(&(point, frame))
    }
    pub(crate) fn canonical_identity_at(&self, point: RecipePoint, owner: Value) -> Option<Value> {
        self.canonical.get(&(point, owner)).copied()
    }
    pub(crate) fn equivalent_identity(&self, point: RecipePoint, a: Value, b: Value) -> bool {
        self.canonical_identity_at(point, a)
            .zip(self.canonical_identity_at(point, b))
            .is_some_and(|(a, b)| a == b)
    }
}

/// Actual compile-time rewrites only; no runtime counters/cache. Threading:
/// compiler owned until publication, then immutable at worker/report handoff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SinkStats {
    pub(crate) numeric_sources: usize,
    pub(crate) numeric_phis: usize,
    pub(crate) cons_sources: usize,
    pub(crate) cons_reads_elided: usize,
    pub(crate) materializations: usize,
    pub(crate) cache_phis: usize,
    pub(crate) analysis_bailed: usize,
}

#[path = "sink_recipes/verify.rs"]
pub(crate) mod verify;
pub(crate) use verify::{roots_at, verify_recipes};
