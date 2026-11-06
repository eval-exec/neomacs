//! Prepared unary immediate-constant callbacks; no generic function entry.
use super::*;
use crate::tagged::gc::function_write_hooks::CallbackParameterKind;

/// Heap/World/era bound metadata. It owns no definition and cannot select a
/// nullary call or authorize a generic callback/source graph.
#[must_use = "retain the prepared unary capability for its matching era"]
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct PreparedUnaryFunction {
    key: SymbolHandle,
    parameter: constant::UnaryParameter,
}

impl PreparedUnaryFunction {
    #[doc(hidden)]
    pub fn symbol(&self) -> &SymbolHandle {
        &self.key
    }
    pub(super) fn key(&self) -> &SymbolHandle {
        &self.key
    }

    /// VIS row: fixture-only mutable key for the stale-handle refusal oracle.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn key_mut_for_test(&mut self) -> &mut SymbolHandle {
        &mut self.key
    }
    pub(super) fn shape(&self) -> CallShape {
        CallShape::UnaryScalar(self.parameter)
    }
}

/// One rule for the rooted immediate argument admitted by these constant bodies.
pub(super) fn check_argument(
    run: &RunAccess<'_, '_>,
    argument: &OwnerRootSlot<'_>,
) -> Result<(), FunctionError> {
    globals::check_home(run, argument)?;
    let value = run
        .owner
        .local
        .as_ref()
        .and_then(|local| local.root_scratch.get(argument.index))
        .ok_or(RegistrationError::StaleRegistration)?
        .value();
    if value.is_nil() || value.is_t() || value.is_fixnum() {
        Ok(())
    } else {
        Err(FunctionError::UnsupportedDefinition)
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) trait EraCoreUnaryExt {
    fn prepare_unary_function_symbol(
        &self,
        key: &SymbolHandle,
    ) -> Result<PreparedUnaryFunction, FunctionError>;
}

impl<K: EraKeeper> EraCoreUnaryExt for EraCore<'_, K> {
    fn prepare_unary_function_symbol(
        &self,
        key: &SymbolHandle,
    ) -> Result<PreparedUnaryFunction, FunctionError> {
        self.prepare_function_checked(key, |initial, lambda, authority| {
            let definition = match initial {
                FunctionCellSnapshot::Bound(value) if !value.is_nil() => value,
                FunctionCellSnapshot::Bound(_)
                | FunctionCellSnapshot::Empty
                | FunctionCellSnapshot::ExplicitlyUnbound => {
                    return Err(FunctionError::UnsupportedDefinition);
                }
            };
            let parameter = constant::unary_candidate(definition, lambda)?;
            // SAFETY: the shared checked constructor holds this backing's
            // exclusive admission writer before any owner may register.
            match unsafe {
                self.registry()
                    .symbols
                    .backing
                    .callback_parameter_kind_at_admission(parameter, authority)
            } {
                CallbackParameterKind::Ordinary => Ok(PreparedUnaryFunction {
                    key: *key,
                    parameter: constant::UnaryParameter(parameter),
                }),
                CallbackParameterKind::Unsupported => Err(FunctionError::UnsupportedDefinition),
            }
        })
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) trait RunAccessUnaryExt<'era> {
    /// Fresh definition, identity and private activation root agree under the
    /// same reader. A source mismatch never starts a callback.
    fn sym_unary_call_begin(
        &mut self,
        callback: &PreparedUnaryFunction,
        expected: &OwnerRootSlot<'era>,
        argument: &OwnerRootSlot<'era>,
        activation: &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, FunctionError>;

    /// Only this capability can publish a replacement with the same prepared
    /// ordinary formal. The default fset remains strictly nullary.
    fn sym_unary_fset_into(
        &mut self,
        callback: &PreparedUnaryFunction,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), FunctionError>;
}

impl<'era> RunAccessUnaryExt<'era> for RunAccess<'_, 'era> {
    fn sym_unary_call_begin(
        &mut self,
        callback: &PreparedUnaryFunction,
        expected: &OwnerRootSlot<'era>,
        argument: &OwnerRootSlot<'era>,
        activation: &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, FunctionError> {
        check_argument(self, argument)?;
        // The sole ordinary formal is unused by this constant body. There is
        // no dynamic call, mutation or observable binding within the kernel.
        self.named_call_begin_with_shape(callback.key(), expected, activation, callback.shape())
    }

    fn sym_unary_fset_into(
        &mut self,
        callback: &PreparedUnaryFunction,
        src: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), FunctionError> {
        self.fset_with_shape(
            &callback.key,
            src,
            dst,
            CallShape::UnaryScalar(callback.parameter),
        )
    }
}

static_assertions::assert_impl_all!(PreparedUnaryFunction: Send, Sync, Copy);
