//! Activation-local native callback dispatch using the legacy funcall protocol.
use super::*;
#[cfg(not(feature = "vm-profile"))]
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

// Callback knobs, read once per process:
// | Knob | Values | Default | Effect |
// | NEOVM_CALLBACK_CACHE | off, on | off | Cache native callback bodies and arity within map/sort/hash/assoc activations; validate after funcall hooks; vm-profile retains legacy dispatch. |

/// Immutable process configuration contains no Lisp state; concurrent mutators
/// may read it. Each callback capability stays with its creating activation.
#[cfg(not(feature = "vm-profile"))]
static CALLBACK_CACHE: LazyLock<bool> =
    LazyLock::new(|| std::env::var("NEOVM_CALLBACK_CACHE").is_ok_and(|value| value == "on"));

#[inline]
pub(crate) fn native_callback_cache_enabled() -> bool {
    #[cfg(not(feature = "vm-profile"))]
    {
        *CALLBACK_CACHE
    }
    // Retain the diagnostic build's legacy metadata-reader population.
    #[cfg(feature = "vm-profile")]
    {
        false
    }
}

/// A conservative process-wide publication clock for the existing native
/// registry. Each mutator owns its registry, static subr objects and
/// activation-local capabilities; a registration on any mutator invalidates all older capabilities. Release
/// publication follows complete registration/reset, and Acquire snapshots precede
/// reading a native body. This does not change the registry's synchronization
/// contract or permit concurrent writes to the same Lisp function object.
static NATIVE_REGISTRATION_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(super) fn publish_native_registration() {
    NATIVE_REGISTRATION_GENERATION
        .fetch_update(Ordering::Release, Ordering::Relaxed, |generation| {
            generation.checked_add(1)
        })
        .expect("native registration generation exhausted");
}

/// Immutable checked native body and arity for one synchronous activation.
/// Contains only a Rust function pointer and a publication snapshot, no heap
/// references, registry loans or effect assumptions. Its native body retains
/// the ordinary &mut Context contract and all ordinary frames and GC roots.
/// Each creating mutator validates its function epoch and this clock after
/// funcall's GC/debug hooks, before invoking the copied body.
#[derive(Clone, Copy)]
pub(crate) struct CheckedNativeCallback {
    body: SubrFn,
    generation: u64,
}

impl CheckedNativeCallback {
    pub(crate) fn resolve(subr: Value, nargs: usize) -> Option<Self> {
        let generation = NATIVE_REGISTRATION_GENERATION.load(Ordering::Acquire);
        let (_, entry) = subr_call_entry_from_value(subr)?;
        if entry.dispatch_kind != SubrDispatchKind::Builtin
            || nargs < usize::from(entry.min_args)
            || entry.max_args.is_some_and(|max| nargs > usize::from(max))
        {
            return None;
        }
        Some(Self {
            body: entry.function?,
            generation,
        })
    }

    #[inline]
    fn is_current(self, ctx: &Context, epoch: u64) -> bool {
        ctx.obarray.function_epoch() == epoch
            && NATIVE_REGISTRATION_GENERATION.load(Ordering::Acquire) == self.generation
            && !ctx.compiler_function_overrides_active()
    }
}

impl Context {
    /// Legacy one-argument funcall with activation-checked body/arity. Callers
    /// must use a proof resolved for one argument and root the original
    /// designator exactly as for apply1_resolved_subr. No new roots are needed:
    /// the cached body contains no Lisp values and native subrs are static.
    #[inline]
    pub(crate) fn apply1_checked_subr(
        &mut self,
        designator: Value,
        subr: Value,
        epoch: u64,
        proof: CheckedNativeCallback,
        arg0: Value,
    ) -> EvalResult {
        self.apply_checked_subr(designator, subr, epoch, proof, &[arg0])
    }

    /// The same protocol for two-argument callbacks, with a proof resolved for
    /// two arguments. Sort supplies its captured object as DESIGNATOR; assoc
    /// and maphash retain the original designator and therefore live cells.
    #[inline]
    pub(crate) fn apply2_checked_subr(
        &mut self,
        designator: Value,
        subr: Value,
        epoch: u64,
        proof: CheckedNativeCallback,
        arg0: Value,
        arg1: Value,
    ) -> EvalResult {
        self.apply_checked_subr(designator, subr, epoch, proof, &[arg0, arg1])
    }

    #[inline]
    fn apply_checked_subr(
        &mut self,
        designator: Value,
        subr: Value,
        epoch: u64,
        proof: CheckedNativeCallback,
        args: &[Value],
    ) -> EvalResult {
        self.maybe_quit_before_gc()?;
        self.enter_interpreted_eval_depth()?;
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(designator, args);
        let result = {
            if self.gc_safe_point_exact_should_collect() {
                self.gc_collect_from_current_roots();
            }
            let entered = match self.take_debug_on_call_arm(DebugOnCallCode::Funcall) {
                Some(arm) => self.do_debug_on_call(arm),
                None => Ok(()),
            };
            match entered {
                Err(flow) => Err(flow),
                Ok(()) => self.maybe_grow_eval_stack(|ctx| {
                    // GNU Ffuncall resolves only after these hooks. A stale
                    // proof falls through within the already-published frame,
                    // without repeating the prologue or retaining old bodies.
                    if !proof.is_current(ctx, epoch) {
                        return ctx.native_callback_miss(designator, subr, epoch, args);
                    }
                    ctx.dispatch_subr_func_unchecked(proof.body, args)
                        .map_err(|flow| ctx.validate_throw(flow))
                }),
            }
        };
        self.depth -= 1;
        self.finish_traced_call(bt_count, result)
    }

    /// A raw registry write changes body/arity without changing the function
    /// epoch. Re-read that mutator's object using only call metadata. Lisp
    /// redefinition or compiler overrides require ordinary name resolution.
    #[cold]
    #[inline(never)]
    fn native_callback_miss(
        &mut self,
        designator: Value,
        subr: Value,
        epoch: u64,
        args: &[Value],
    ) -> EvalResult {
        if self.obarray.function_epoch() != epoch || self.compiler_function_overrides_active() {
            return self.funcall_general_untraced(designator, LispArgVec::from_slice(args));
        }
        let Some((symbol, entry)) = subr_call_entry_from_value(subr) else {
            return Err(signal(LispCondition::InvalidFunction, vec![designator]));
        };
        self.apply_subr_object_with_entry(symbol, subr, args, entry)
    }
}

#[cfg(test)]
#[path = "tests/native_callback_cache.rs"]
mod native_callback_cache;
