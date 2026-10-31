//! Whole-function placement without changing emitted bytes.
//!
//! Unset `NEOVM_JIT_CODE_PLACEMENT` is `switch64`. `off` is the explicit opt-out.
//!
//! Configuration is immutable process state. The allocation hint is a scalar
//! owned by one synchronous compiler invocation on its current thread; it never
//! contains Lisp state and is consumed before any compiled code is published.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::OnceLock;

use cranelift_module::{FuncId, Module, ModuleReloc};

use super::CompileError;
use crate::emacs_core::jit::backend::BackendError;

pub(crate) mod anchors;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod replay;

/// One cache-line-relative function entry position, validated at construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlacementOffset(u8);

impl PlacementOffset {
    const ZERO: Self = Self(0);

    pub(crate) fn new(offset: usize) -> Result<Self, PlacementError> {
        if offset < 64 {
            Ok(Self(offset as u8))
        } else {
            Err(PlacementError::OffsetOutOfRange(offset.to_string()))
        }
    }

    pub(crate) fn bytes(self) -> usize {
        usize::from(self.0)
    }

    pub(crate) fn validate_alignment(self, alignment: u64) -> Result<Self, PlacementError> {
        if !alignment.is_power_of_two() {
            return Err(PlacementError::InvalidAlignment(alignment));
        }
        if u64::from(self.0) % alignment != 0 {
            return Err(PlacementError::IncompatibleAlignment {
                offset: self.0,
                alignment,
            });
        }
        Ok(self)
    }
}

/// A whole-function placement rule. No runtime execution state is retained.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, strum::EnumString)]
pub(crate) enum PlacementPolicy {
    #[default]
    #[strum(serialize = "off")]
    Off,
    #[strum(serialize = "function64")]
    Function64,
    #[strum(serialize = "loop64")]
    Loop64,
    #[strum(serialize = "switch64")]
    Switch64,
}

/// Process configuration; an exact diagnostic offset excludes a policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacementConfig {
    Policy(PlacementPolicy),
    Offset(PlacementOffset),
}

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum PlacementError {
    #[error("invalid NEOVM_JIT_CODE_PLACEMENT value: {0}")]
    InvalidPolicy(String),
    #[error("NEOVM_JIT_CODE_OFFSET must be an integer from 0 through 63: {0}")]
    OffsetOutOfRange(String),
    #[error("NEOVM_JIT_CODE_OFFSET requires NEOVM_JIT_CODE_PLACEMENT=off")]
    ConflictingConfiguration,
    #[error("invalid code alignment: {0}")]
    InvalidAlignment(u64),
    #[error("code offset {offset} is incompatible with required alignment {alignment}")]
    IncompatibleAlignment { offset: u8, alignment: u64 },
    #[error("code placement requires the JIT arena memory provider")]
    UnsupportedProvider,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[error("diagnostic code replay refused: {0}")]
    Replay(#[from] replay::ReplayError),
}

pub(crate) fn parse_config(
    policy: Option<&str>,
    offset: Option<&str>,
) -> Result<PlacementConfig, PlacementError> {
    let policy = match policy {
        // Graduated with `NEOVM_JIT_SWITCH_HOIST`. `off` remains the opt-out.
        None => PlacementPolicy::Switch64,
        Some(value) => value
            .trim()
            .parse()
            .map_err(|_| PlacementError::InvalidPolicy(value.to_owned()))?,
    };
    match offset {
        None => Ok(PlacementConfig::Policy(policy)),
        Some(value) => {
            if policy != PlacementPolicy::Off {
                return Err(PlacementError::ConflictingConfiguration);
            }
            let offset = value
                .trim()
                .parse::<usize>()
                .map_err(|_| PlacementError::OffsetOutOfRange(value.to_owned()))?;
            Ok(PlacementConfig::Offset(PlacementOffset::new(offset)?))
        }
    }
}

pub(crate) fn config() -> Result<PlacementConfig, PlacementError> {
    #[cfg(test)]
    if let Some(configuration) = CONFIG_OVERRIDE.with(Cell::get) {
        return Ok(configuration);
    }
    static CONFIG: OnceLock<Result<PlacementConfig, PlacementError>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            parse_config(
                std::env::var("NEOVM_JIT_CODE_PLACEMENT").ok().as_deref(),
                std::env::var("NEOVM_JIT_CODE_OFFSET").ok().as_deref(),
            )
        })
        .clone()
}

/// Diagnostics can select one Lisp name from the existing perf-label format.
/// The immutable selector uses the same sanitized, length-limited name as the
/// assembly dump; normal placement policies always apply to all JIT leaves.
pub(crate) fn diagnostic_function_matches(label: Option<&str>) -> bool {
    static FUNCTION: OnceLock<Option<String>> = OnceLock::new();
    let selector = FUNCTION.get_or_init(|| std::env::var("NEOVM_JIT_CODE_FUNCTION").ok());
    selector_matches(selector.as_deref(), label)
}

pub(crate) fn selector_matches(selector: Option<&str>, label: Option<&str>) -> bool {
    match selector {
        None => true,
        Some(selector) => label
            .and_then(crate::emacs_core::jit::stats::perf_map::label_parts)
            .is_some_and(|(name, _)| name == selector),
    }
}

#[cfg(test)]
thread_local! {
    /// Compiler-test scalar only; configuration never holds mutator state.
    static CONFIG_OVERRIDE: Cell<Option<PlacementConfig>> = const { Cell::new(None) };
}

#[cfg(test)]
#[must_use]
pub(crate) fn config_scope_for_test(configuration: PlacementConfig) -> impl Drop {
    #[must_use]
    struct Scope {
        previous: Option<PlacementConfig>,
        _compiler_thread: PhantomData<Rc<()>>,
    }
    static_assertions::assert_not_impl_any!(Scope: Send, Sync);
    impl Drop for Scope {
        fn drop(&mut self) {
            let _ = CONFIG_OVERRIDE.try_with(|current| current.set(self.previous));
        }
    }
    Scope {
        previous: CONFIG_OVERRIDE.with(|current| current.replace(Some(configuration))),
        _compiler_thread: PhantomData,
    }
}

/// Only loop placement requests machine CFG metadata; off preserves all flags.
pub(crate) fn needs_machine_cfg() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
        && matches!(
            config(),
            Ok(PlacementConfig::Policy(PlacementPolicy::Loop64))
        )
}

/// Select the shortest backward machine-CFG edge's target as an approximation
/// of an innermost loop header. Ties use the earlier header deterministically.
/// The policy rounds down to a legal entry offset; diagnostics require exact
/// compatibility, so an unsupported offset cannot silently become another one.
pub(crate) fn placement_for(
    config: PlacementConfig,
    alignment: u64,
    edges: &[(u32, u32)],
) -> Result<Option<PlacementOffset>, PlacementError> {
    if !alignment.is_power_of_two() {
        return Err(PlacementError::InvalidAlignment(alignment));
    }
    let desired = match config {
        PlacementConfig::Offset(offset) => {
            return offset.validate_alignment(alignment).map(Some);
        }
        PlacementConfig::Policy(policy) => match policy {
            PlacementPolicy::Off => return Ok(None),
            PlacementPolicy::Function64 => 0,
            PlacementPolicy::Switch64 => {
                return anchors::placement_for_anchor(alignment, None).map(Some);
            }
            PlacementPolicy::Loop64 => edges
                .iter()
                .filter(|(from, to)| to <= from)
                .min_by_key(|(from, to)| (from - to, *to))
                .map_or(0, |(_, to)| (64 - u64::from(*to) % 64) % 64),
        },
    };
    let rounded = desired & !(alignment - 1);
    Ok(Some(PlacementOffset::new(rounded as usize)?))
}

thread_local! {
    /// Compiler-only, one-shot scalar; neither Lisp state nor an address cache.
    static ALLOCATION_HINT: Cell<Option<PlacementOffset>> = const { Cell::new(None) };
}

/// Restores the enclosing compiler invocation's hint on success or failure.
/// Confined to the thread that stages and consumes the allocation request.
#[must_use]
pub(crate) struct PlacementScope {
    previous: Option<PlacementOffset>,
    _compiler_thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(PlacementScope: Send, Sync);

impl PlacementScope {
    pub(crate) fn stage(offset: PlacementOffset) -> Self {
        Self {
            previous: ALLOCATION_HINT.with(|hint| hint.replace(Some(offset))),
            _compiler_thread: PhantomData,
        }
    }

    fn consumed(&self) -> bool {
        ALLOCATION_HINT.with(|hint| hint.get().is_none())
    }

    /// A successful definition either consumed this request or used another
    /// provider. Dropping this scope restores any enclosing compiler's hint.
    fn finish(self, configuration: PlacementConfig) -> Result<(), PlacementError> {
        if !self.consumed() {
            match configuration {
                PlacementConfig::Offset(_) => return Err(PlacementError::UnsupportedProvider),
                PlacementConfig::Policy(_) => {
                    static WARNED: OnceLock<()> = OnceLock::new();
                    WARNED.get_or_init(|| {
                        tracing::warn!(
                            target: "neovm_jit",
                            "JIT placement unavailable without the arena; retaining ordinary placement"
                        );
                    });
                }
            }
        }
        Ok(())
    }
}

impl Drop for PlacementScope {
    fn drop(&mut self) {
        let _ = ALLOCATION_HINT.try_with(|hint| hint.set(self.previous));
    }
}

pub(crate) fn take_allocation_hint() -> Option<PlacementOffset> {
    ALLOCATION_HINT.with(|hint| hint.take())
}

/// Compile once, then let the arena choose the code buffer's address. Internal
/// relative branches and all relocation offsets retain their emitted values.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn define_placed_function<M: Module>(
    module: &mut M,
    fid: FuncId,
    ctx: &mut cranelift_codegen::Context,
    configuration: PlacementConfig,
) -> Result<(), CompileError> {
    let failure = |error: String| {
        tracing::error!(target: "neovm_jit", %error, "JIT code placement failed");
        CompileError::Backend(BackendError::Define(error))
    };
    // No post-codegen metadata is needed for entry alignment or a function
    // with no source locations at all. Preserve Cranelift's owned relocation
    // path instead of collecting a vector that define_function_bytes clones.
    let entry_only = match configuration {
        PlacementConfig::Policy(PlacementPolicy::Function64) => true,
        PlacementConfig::Policy(PlacementPolicy::Switch64) => ctx.func.srclocs.is_empty(),
        PlacementConfig::Policy(PlacementPolicy::Off | PlacementPolicy::Loop64)
        | PlacementConfig::Offset(_) => false,
    };
    if entry_only {
        let scope = PlacementScope::stage(PlacementOffset::ZERO);
        module
            .define_function(fid, ctx)
            .map_err(|error| failure(error.to_string()))?;
        return scope
            .finish(configuration)
            .map_err(|error| failure(error.to_string()));
    }
    if let PlacementConfig::Offset(_) = configuration {
        // Exact byte-offset replay validates every instruction in trusted
        // Cranelift VCode before relaxing constant-pool buffer alignment.
        ctx.set_disasm(true);
    }
    ctx.compile(module.isa(), &mut Default::default())
        .map_err(|error| failure(error.inner.to_string()))?;
    let code = ctx
        .compiled_code()
        .ok_or_else(|| failure("Cranelift returned no compiled code".to_owned()))?;
    let validated_replay = match configuration {
        PlacementConfig::Offset(_) => Some(
            replay::ValidatedReplay::validate(code, module.isa())
                .map_err(|error| failure(PlacementError::from(error).to_string()))?,
        ),
        PlacementConfig::Policy(_) => None,
    };
    let alignment = match &validated_replay {
        Some(replay) => replay.allocation_alignment(),
        None => u64::from(code.buffer.alignment)
            .max(u64::from(module.isa().function_alignment().minimum))
            .max(module.isa().symbol_alignment()),
    };
    let offset = match configuration {
        PlacementConfig::Policy(PlacementPolicy::Switch64) => anchors::placement_for_anchor(
            alignment,
            anchors::first_anchor_offset(code, anchors::PlacementAnchor::SwitchConsGuard),
        )
        .map_err(|error| failure(error.to_string()))?,
        PlacementConfig::Policy(
            PlacementPolicy::Off | PlacementPolicy::Function64 | PlacementPolicy::Loop64,
        )
        | PlacementConfig::Offset(_) => placement_for(configuration, alignment, &code.bb_edges)
            .map_err(|error| failure(error.to_string()))?
            .ok_or_else(|| failure("code placement requested with the off policy".to_owned()))?,
    };
    let relocations: Vec<_> = code
        .buffer
        .relocs()
        .iter()
        .map(|reloc| ModuleReloc::from_mach_reloc(reloc, &ctx.func, fid))
        .collect();
    let scope = PlacementScope::stage(offset);
    let bytes = match &validated_replay {
        Some(replay) => replay.code_buffer(),
        None => code.code_buffer(),
    };
    module
        .define_function_bytes(fid, alignment, bytes, &relocations)
        .map_err(|error| failure(error.to_string()))?;
    scope
        .finish(configuration)
        .map_err(|error| failure(error.to_string()))
}

#[cfg(test)]
#[path = "code_placement/tests/placement_test.rs"]
mod tests;

/// Carrier park: clear the code-placement hint.
pub(crate) fn reset_carrier_tls() {
    ALLOCATION_HINT.with(|slot| slot.set(None));
}

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    crate::tls_fingerprint::mix(h, ALLOCATION_HINT.with(|slot| slot.get().is_some() as u64));
}
