//! Compiler-side implementation of the value layer's cold function hooks.
use super::Baton;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::symbol::FunctionEpochBump;
use crate::tagged::gc::function_write_hooks::{
    CompilerActivity, FunctionWriteHooks, InitialDefinition,
};
use crate::tagged::gc::{KeeperSymbols, SymbolWriteToken};
use crate::tagged::value::TaggedValue;

impl FunctionWriteHooks for Baton {
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
