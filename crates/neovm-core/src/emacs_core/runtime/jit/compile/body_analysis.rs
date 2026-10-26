//! Exact-source CFG ownership for one synchronous mutator compilation.
//! Successful Switch-free analysis is reusable; live Switch observations and
//! every error remain local to their original consumer. Nothing escapes into
//! a compiled leaf or a backend worker.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::rc::Rc;

use super::{Cfg, CompileError, JitParamShape, analyze_cfg, mask_dynamic_prefix};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::inline::FusedBody;
use crate::emacs_core::value::Value;

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub(crate) struct EntryStackDepth(usize);

impl From<usize> for EntryStackDepth {
    fn from(depth: usize) -> Self {
        Self(depth)
    }
}

impl From<JitParamShape> for EntryStackDepth {
    fn from(shape: JitParamShape) -> Self {
        Self(shape.entry_depth())
    }
}

impl From<EntryStackDepth> for usize {
    fn from(depth: EntryStackDepth) -> Self {
        depth.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct SourceOpIndex(usize);

impl From<usize> for SourceOpIndex {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

impl From<SourceOpIndex> for usize {
    fn from(index: SourceOpIndex) -> Self {
        index.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum EntryPoint {
    Function,
    Osr(SourceOpIndex),
}

impl From<Option<usize>> for EntryPoint {
    fn from(pc: Option<usize>) -> Self {
        match pc {
            None => Self::Function,
            Some(pc) => Self::Osr(SourceOpIndex::from(pc)),
        }
    }
}

impl From<EntryPoint> for Option<usize> {
    fn from(entry: EntryPoint) -> Self {
        match entry {
            EntryPoint::Function => None,
            EntryPoint::Osr(pc) => Some(usize::from(pc)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum TargetAddressing<'s> {
    InstructionIndices,
    GnuByteOffsets(&'s [GnuByteOffsetMapEntry]),
}

impl<'s> From<Option<&'s [GnuByteOffsetMapEntry]>> for TargetAddressing<'s> {
    fn from(map: Option<&'s [GnuByteOffsetMapEntry]>) -> Self {
        match map {
            None => Self::InstructionIndices,
            Some(map) => Self::GnuByteOffsets(map),
        }
    }
}

impl<'s> From<TargetAddressing<'s>> for Option<&'s [GnuByteOffsetMapEntry]> {
    fn from(addressing: TargetAddressing<'s>) -> Self {
        match addressing {
            TargetAddressing::InstructionIndices => None,
            TargetAddressing::GnuByteOffsets(map) => Some(map),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
struct CapturedSlots(NonZeroUsize);

impl From<NonZeroUsize> for CapturedSlots {
    fn from(slots: NonZeroUsize) -> Self {
        Self(slots)
    }
}

impl From<CapturedSlots> for usize {
    fn from(slots: CapturedSlots) -> Self {
        slots.0.get()
    }
}

#[derive(Clone, Copy, Debug)]
enum ConstantPrefix {
    Unpatched,
    Captured(CapturedSlots),
}

impl From<usize> for ConstantPrefix {
    fn from(slots: usize) -> Self {
        match NonZeroUsize::new(slots) {
            None => Self::Unpatched,
            Some(slots) => Self::Captured(CapturedSlots::from(slots)),
        }
    }
}

impl From<ConstantPrefix> for usize {
    fn from(prefix: ConstantPrefix) -> Self {
        match prefix {
            ConstantPrefix::Unpatched => 0,
            ConstantPrefix::Captured(slots) => usize::from(slots),
        }
    }
}

/// Hydrate canonical ops at the original pre-parameter-validation frontier.
/// Preparation later owns the existing prefix mask, without resolving twice.
pub(super) struct FunctionView<'s> {
    function: &'s ByteCodeFunction,
    ops: &'s [Op],
    _mutator_only: PhantomData<Rc<()>>,
}

impl<'s> FunctionView<'s> {
    pub(super) fn new(function: &'s ByteCodeFunction) -> Self {
        Self {
            function,
            ops: function.executable_ops(),
            _mutator_only: PhantomData,
        }
    }
}

/// Private fields prevent a consumer from pairing an unrelated CFG and body.
/// The marker forbids sending interior Lisp values or this cell to a worker.
pub(crate) struct CompileBody<'s> {
    ops: &'s [Op],
    constants: Cow<'s, [Value]>,
    targets: TargetAddressing<'s>,
    depth: EntryStackDepth,
    prefix: ConstantPrefix,
    stable: OnceCell<StableCfg>,
    _mutator_only: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for CompileBody<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompileBody")
            .field("ops", &self.ops.len())
            .field("constants", &self.constants.len())
            .field("targets", &self.targets)
            .field("depth", &self.depth)
            .field("prefix", &self.prefix)
            .field("analyzed", &self.stable.get().is_some())
            .finish()
    }
}

impl<'s> CompileBody<'s> {
    /// Legacy tuple APIs retain their signatures; only this boundary imports
    /// their existing source addressing/depth conventions.
    pub(crate) fn from_legacy_parts(
        ops: &'s [Op],
        constants: &'s [Value],
        depth: usize,
        offsets: Option<&'s [GnuByteOffsetMapEntry]>,
        prefix: usize,
    ) -> Self {
        Self {
            ops,
            constants: Cow::Borrowed(constants),
            targets: TargetAddressing::from(offsets),
            depth: EntryStackDepth::from(depth),
            prefix: ConstantPrefix::from(prefix),
            stable: OnceCell::new(),
            _mutator_only: PhantomData,
        }
    }

    /// Runs the existing first prefix mask at the same frontend frontier.
    pub(super) fn for_function(view: FunctionView<'s>, shape: JitParamShape) -> Self {
        let function = view.function;
        let prefix = function.jit_runtime().patched_prefix();
        let constants: Cow<'s, [Value]> = if prefix > 0 {
            Cow::Owned(mask_dynamic_prefix(&function.constants, prefix))
        } else {
            Cow::Borrowed(&function.constants)
        };
        Self {
            ops: view.ops,
            constants,
            targets: TargetAddressing::from(function.executable_gnu_byte_offset_map()),
            depth: EntryStackDepth::from(shape),
            prefix: ConstantPrefix::from(prefix),
            stable: OnceCell::new(),
            _mutator_only: PhantomData,
        }
    }

    /// Preserve the old MIR/classic-fuser input map exactly, including the
    /// pre-existing early-fused route. Final lowering uses for_fused instead.
    pub(super) fn for_early_fused<'b>(original: &Self, fused: &'b FusedBody) -> CompileBody<'b>
    where
        's: 'b,
    {
        CompileBody {
            ops: &fused.ops,
            constants: Cow::Borrowed(&fused.constants),
            targets: original.targets,
            depth: original.depth,
            prefix: original.prefix,
            stable: OnceCell::new(),
            _mutator_only: PhantomData,
        }
    }

    /// Any Some(fusion) is a new exact view; it never imports the old cell.
    pub(super) fn for_fused<'b>(original: &Self, fused: &'b FusedBody) -> CompileBody<'b> {
        CompileBody {
            ops: &fused.ops,
            constants: Cow::Borrowed(&fused.constants),
            targets: TargetAddressing::from(fused.offset_map.as_deref()),
            depth: original.depth,
            prefix: original.prefix,
            stable: OnceCell::new(),
            _mutator_only: PhantomData,
        }
    }

    pub(crate) fn ops(&self) -> &[Op] {
        self.ops
    }
    pub(crate) fn constants(&self) -> &[Value] {
        &self.constants
    }
    pub(crate) fn entry_depth(&self) -> EntryStackDepth {
        self.depth
    }
    pub(crate) fn as_offset_map(&self) -> Option<&[GnuByteOffsetMapEntry]> {
        self.targets.into()
    }
    pub(super) fn as_prefix_slots(&self) -> usize {
        self.prefix.into()
    }

    /// Preserve the second lower-seam prefix mask. A new mask is a new owner,
    /// even when the effective values happen to compare equal.
    pub(super) fn for_lowering(&self) -> LowerBody<'_, 's> {
        match self.prefix {
            ConstantPrefix::Unpatched => LowerBody::Unchanged(self),
            ConstantPrefix::Captured(slots) => LowerBody::Remasked(CompileBody {
                ops: self.ops,
                constants: Cow::Owned(mask_dynamic_prefix(self.constants(), usize::from(slots))),
                targets: self.targets,
                depth: self.depth,
                prefix: self.prefix,
                stable: OnceCell::new(),
                _mutator_only: PhantomData,
            }),
        }
    }

    pub(crate) fn source_cfg(&self) -> Result<CfgLease<'_, 's>, CompileError> {
        if let Some(cfg) = self.stable.get() {
            return Ok(CfgLease::Shared(SharedCfg { body: self, cfg }));
        }
        match StableCfg::analyze(self)? {
            SuccessfulCfg::Stable(cfg) => {
                let cfg = self.stable.get_or_init(|| cfg);
                Ok(CfgLease::Shared(SharedCfg { body: self, cfg }))
            }
            SuccessfulCfg::FreshSwitch(cfg) => {
                Ok(CfgLease::FreshSwitch(FreshSwitchCfg { body: self, cfg }))
            }
        }
    }
}

pub(super) enum LowerBody<'o, 's> {
    Unchanged(&'o CompileBody<'s>),
    Remasked(CompileBody<'s>),
}

impl<'s> AsRef<CompileBody<'s>> for LowerBody<'_, 's> {
    fn as_ref(&self) -> &CompileBody<'s> {
        match self {
            Self::Unchanged(body) => body,
            Self::Remasked(body) => body,
        }
    }
}

struct StableCfg {
    cfg: Cfg,
}
enum SuccessfulCfg {
    Stable(StableCfg),
    FreshSwitch(Cfg),
}

impl StableCfg {
    /// Sole validating constructor. Canonical pass1 inserts a key for EVERY
    /// Switch, including empty tables/unreachable code, before reachability;
    /// it returns this map unmodified. No sibling can supply an arbitrary Cfg.
    fn analyze(body: &CompileBody<'_>) -> Result<SuccessfulCfg, CompileError> {
        let cfg = analyze_cfg(
            body.ops(),
            body.constants(),
            body.targets.into(),
            usize::from(body.depth),
        )?;
        Ok(if cfg.switch_targets.is_empty() {
            SuccessfulCfg::Stable(Self { cfg })
        } else {
            SuccessfulCfg::FreshSwitch(cfg)
        })
    }
}

pub(crate) struct SharedCfg<'o, 's> {
    body: &'o CompileBody<'s>,
    cfg: &'o StableCfg,
}
pub(crate) struct FreshSwitchCfg<'o, 's> {
    body: &'o CompileBody<'s>,
    cfg: Cfg,
}
pub(crate) enum CfgLease<'o, 's> {
    Shared(SharedCfg<'o, 's>),
    FreshSwitch(FreshSwitchCfg<'o, 's>),
}

impl<'s> CfgLease<'_, 's> {
    fn body(&self) -> &CompileBody<'s> {
        match self {
            Self::Shared(source) => source.body,
            Self::FreshSwitch(source) => source.body,
        }
    }
    pub(crate) fn ops(&self) -> &[Op] {
        self.body().ops()
    }
    pub(crate) fn constants(&self) -> &[Value] {
        self.body().constants()
    }
    pub(crate) fn entry_depth(&self) -> EntryStackDepth {
        self.body().entry_depth()
    }
}

impl std::fmt::Debug for CfgLease<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shared(source) => f.debug_tuple("Shared").field(source.body).finish(),
            Self::FreshSwitch(source) => f.debug_tuple("FreshSwitch").field(source.body).finish(),
        }
    }
}

impl AsRef<Cfg> for CfgLease<'_, '_> {
    fn as_ref(&self) -> &Cfg {
        match self {
            Self::Shared(source) => &source.cfg.cfg,
            Self::FreshSwitch(source) => &source.cfg,
        }
    }
}

static_assertions::assert_eq_size!(EntryStackDepth, usize);
static_assertions::assert_eq_size!(SourceOpIndex, usize);
static_assertions::assert_eq_size!(CapturedSlots, NonZeroUsize);

static_assertions::assert_not_impl_any!(FunctionView<'static>: Send, Sync);
static_assertions::assert_not_impl_any!(CompileBody<'static>: Send, Sync);
static_assertions::assert_not_impl_any!(CfgLease<'static, 'static>: Send, Sync);

#[cfg(test)]
#[path = "tests/body_analysis.rs"]
mod tests;
