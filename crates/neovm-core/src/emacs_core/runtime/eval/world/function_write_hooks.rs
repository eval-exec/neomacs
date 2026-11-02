//! Compiler-side implementation of the value layer's cold function hooks.
use super::Baton;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::subr_abi::SubrValueExt;
use crate::emacs_core::eval::subr_abi::{SubrFn, SubrFn2, SubrObj};
use crate::emacs_core::intern::{SymId, lookup_interned};
use crate::emacs_core::symbol::FunctionEpochBump;
use crate::tagged::gc::function_write_hooks::{
    CanonicalHofSeed, CompilerActivity, FunctionWriteHooks, HofOperation, HofPreparationError,
    HofRegistrationEpoch, InitialDefinition,
};
use crate::tagged::gc::{KeeperSymbols, SymbolWriteToken};
use crate::tagged::header::{SubrDispatchKind, VecLikeType};
use crate::tagged::value::TaggedValue;

impl FunctionWriteHooks for Baton {
    unsafe fn prepare_hof_builtin(
        &self,
        operation: HofOperation,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<CanonicalHofSeed, HofPreparationError> {
        let registration_clock = super::super::callback::native_registration_clock();
        let registration_epoch = registration_clock.snapshot();
        let prepared: Result<_, HofPreparationError> = (|| {
            let spelling = match operation {
                HofOperation::Mapc => "mapc",
                HofOperation::Mapcar => "mapcar",
            };
            let symbol =
                lookup_interned(spelling).ok_or(HofPreparationError::MissingCanonicalSymbol)?;
            let registered = super::super::lookup_global_subr_entry(symbol)
                .ok_or(HofPreparationError::UnsupportedBuiltin)?;
            verify_hof_entry(operation, &registered)?;
            // This cold canonical lookup is independent of the mutable function
            // cell. Its existing keeper TLS cache is never consulted by selection.
            let canonical = <TaggedValue as SubrValueExt>::subr_from_sym_id(symbol);
            // Preparation excludes registration updates by contract. Also use the
            // existing static-entry reader guard in JIT builds: test registration
            // writers and selected production writers honor this same lock.
            #[cfg(feature = "jit")]
            let actual = crate::tagged::value::with_static_subr_entry_read(|| {
                // SAFETY: canonical names the initialized leaked static object;
                // this reader guard and exclusive preparation exclude metadata
                // writers throughout the copy. No reference escapes the helper.
                unsafe { copy_static_hof_entry(canonical) }
            })
            .ok_or(HofPreparationError::SubrMetadataBusy)?;
            #[cfg(not(feature = "jit"))]
            // SAFETY: canonical names the initialized leaked static object and
            // exclusive preparation excludes registration writers for the copy.
            let actual = unsafe { copy_static_hof_entry(canonical) };
            let (actual_symbol, actual_entry) =
                actual.ok_or(HofPreparationError::UnsupportedBuiltin)?;
            if actual_symbol != symbol {
                return Err(HofPreparationError::UnsupportedBuiltin);
            }
            verify_hof_entry(operation, &actual_entry)?;
            Ok((symbol, canonical))
        })();
        // The original observation brackets every registry/metadata read,
        // including refused paths. A completed concurrent publication makes
        // the whole preparation retryable rather than issuing a mixed receipt.
        if registration_clock.snapshot() != registration_epoch {
            return Err(HofPreparationError::SubrMetadataBusy);
        }
        let (symbol, canonical) = prepared?;
        // SAFETY: the exclusive checked admission caller retains this backing
        // before registration, with the original keeper's canonical-subr table
        // installed and no concurrent write overlapping unsafe metadata reads.
        // The exhaustive spelling choice and canonical constructor above
        // provide the actual leaked static subr, never a current cell snapshot.
        // Both the registration and canonical object's metadata were checked
        // against the original fixed builtin body and two-argument arity.
        // The original registration stamp was confirmed unchanged afterward;
        // running selection must compare it again under its symbol reader.
        // The seed exposes comparison only, not an entry or callable metadata.
        Ok(unsafe {
            CanonicalHofSeed::from_canonical_static_subr(
                operation,
                symbol,
                canonical,
                registration_epoch,
                registration_clock,
            )
        })
    }

    unsafe fn classify_initial_definition(
        &self,
        value: TaggedValue,
        _authority: &SymbolWriteToken<'_>,
    ) -> InitialDefinition {
        if opaque_initial_bytecode(value) {
            InitialDefinition::OpaqueScalarConstant
        } else {
            InitialDefinition::Unsupported
        }
    }

    fn check_compiler_quiescent(&self) -> Result<(), CompilerActivity> {
        #[cfg(feature = "jit")]
        {
            if crate::emacs_core::jit::cache::native_depth() != 0 {
                return Err(CompilerActivity::NativeActivation);
            }
            if crate::emacs_core::jit::bg::pending_count() != 0 {
                return Err(CompilerActivity::BackgroundCompilation);
            }
        }
        Ok(())
    }

    unsafe fn reconcile_function_write(&self, symbol: SymId, _keeper: &KeeperSymbols<'_>) {
        #[cfg(feature = "jit")]
        crate::emacs_core::jit::cache::evict_inline_dependents(symbol);
        super::super::note_function_epoch_move(FunctionEpochBump::InternalCellWrite, Some(symbol));
    }
}

/// Copy only cold verification metadata, without a call-path accounting hook
/// or a TLS registry lookup. No reference into the static object is returned.
///
/// # Safety
/// `canonical` carries real provenance for an initialized leaked canonical
/// static subr object. Its header remains live and registration writers are
/// excluded throughout this copy, including by the existing reader guard when
/// applicable. This copy is not a live registration proof; the stamped receipt
/// must be revalidated at the running selection boundary.
unsafe fn copy_static_hof_entry(
    canonical: TaggedValue,
) -> Option<(SymId, super::super::SubrEntry)> {
    let pointer = canonical.as_veclike_ptr()?;
    // SAFETY: the caller retains the initialized static allocation and its
    // header; checking its actual type precedes the full-object projection.
    if unsafe { (*pointer).type_tag } != VecLikeType::Subr {
        return None;
    }
    // SAFETY: the initialized header identifies SubrObj, the allocation has
    // static lifetime, and the caller excludes all metadata writers here.
    let subr = unsafe { &*pointer.cast::<SubrObj>() };
    Some((
        subr.metadata().symbol_id(),
        super::super::SubrEntry {
            function: subr.function(),
            min_args: subr.metadata().min_args(),
            max_args: subr.metadata().max_args(),
            dispatch_kind: subr.metadata().dispatch_kind(),
            interactive_spec: None,
        },
    ))
}

/// Check identity of the original implementation, not merely a builtin label
/// that native re-registration could reuse. No inspected entry is executed.
fn verify_hof_entry(
    operation: HofOperation,
    entry: &super::super::SubrEntry,
) -> Result<(), HofPreparationError> {
    let expected: SubrFn2 = match operation {
        HofOperation::Mapc => crate::emacs_core::builtins::builtin_mapc_2,
        HofOperation::Mapcar => crate::emacs_core::builtins::builtin_mapcar_2,
    };
    let Some(SubrFn::A2(actual)) = entry.function else {
        return Err(HofPreparationError::UnsupportedBuiltin);
    };
    if entry.dispatch_kind != SubrDispatchKind::Builtin
        || entry.min_args != 2
        || entry.max_args != Some(2)
        || !std::ptr::fn_addr_eq(actual, expected)
    {
        return Err(HofPreparationError::UnsupportedBuiltin);
    }
    Ok(())
}

/// Existing compiled constants are opaque replacement targets, never callable
/// through the restricted interpreter. This is the sole shape recognizer.
fn opaque_initial_bytecode(value: TaggedValue) -> bool {
    value.bytecode_data_if_materialized().is_some_and(|f| {
        f.lexical
            && f.params.fixed_arity() == Some(0)
            && f.arglist.is_nil()
            && f.ops_sealed
            && f.stack_verified
            && f.ops.as_slice() == [Op::Constant(0), Op::Return]
            && f.constants.len() == 1
            && f.constants
                .first()
                .is_some_and(|answer| answer.is_nil() || answer.is_t() || answer.is_fixnum())
            && f.env.is_none()
            && f.interactive.is_none()
            && f.doc_form.is_none()
            && f.docstring.is_none()
            && f.extra_slots.is_empty()
            && f.gnu_bytecode_bytes.is_none()
            && f.gnu_byte_offset_map.is_none()
            && f.lazy_gnu_code.is_none()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn nmjit_n4_function_write_hooks_source_guard_test() {
        let lower = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/tagged/gc/function_write_hooks.rs"
        ));
        assert!(!lower.contains("crate::emacs_core"));
        assert!(!lower.contains("type Symbol:"));
        assert!(
            !lower.contains("AtomicU64::new"),
            "observer must reuse the existing publisher"
        );
        let implementation = include_str!("function_write_hooks.rs");
        assert!(
            implementation
                .lines()
                .any(|line| line == "impl FunctionWriteHooks for Baton {")
        );
        assert!(
            !implementation
                .lines()
                .any(|line| line.starts_with("impl FunctionWriteHooks for ")
                    && line != "impl FunctionWriteHooks for Baton {")
        );
        let keeper = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/tagged/gc/mutator_registration/keeper.rs"
        ));
        assert!(keeper.contains("FunctionWriteHooks"));

        let writer = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/tagged/gc/mutator_registration/symbol_authority/functions/write.rs"
        ));
        for upward in [
            "crate::emacs_core::bytecode",
            "crate::emacs_core::jit",
            "crate::emacs_core::eval",
            "FunctionEpochBump",
            "fn opaque_initial_bytecode",
            "fn keeper_compiler_quiescent",
        ] {
            assert!(
                !writer.contains(upward),
                "function-write hook edge: {upward}"
            );
        }
        assert!(writer.contains("classify_initial_definition"));
        assert!(writer.contains("check_compiler_quiescent"));
        assert!(writer.contains("reconcile_function_write"));
    }
}
