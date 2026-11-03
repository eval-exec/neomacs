//! Frozen mixed-executor inputs, checked only through stopped symbol authority.
//! This does not widen either C3's recursive graph closure or C5's name tape.
use super::*;
use crate::tagged::value::TaggedValue as Value;
use std::num::NonZeroU32;

mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tagged::gc::mutator_registration) enum CombinedInputRole {
    PlainScalar,
    NameDefinition,
    ScalarDefinition,
    DirectName,
}

/// A request, not evidence: the private receipt below is minted by the keeper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tagged::gc::mutator_registration) struct CombinedInput {
    key: SymbolHandle,
    role: CombinedInputRole,
}
impl CombinedInput {
    pub(in crate::tagged::gc::mutator_registration) fn new(
        key: SymbolHandle,
        role: CombinedInputRole,
    ) -> Self {
        Self { key, role }
    }

    pub(in crate::tagged::gc::mutator_registration) fn key(self) -> SymbolHandle {
        self.key
    }

    pub(in crate::tagged::gc::mutator_registration) fn role(self) -> CombinedInputRole {
        self.role
    }
}

/// Number of input roots inspected, distinct from program or contention limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tagged::gc::mutator_registration) struct CombinedInputLimit(NonZeroU32);

impl TryFrom<usize> for CombinedInputLimit {
    type Error = CombinedInputError;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        let value = u32::try_from(value).map_err(|_| CombinedInputError::InvalidLimit)?;
        if value > u32::from(u16::MAX) + 1 {
            return Err(CombinedInputError::InvalidLimit);
        }
        NonZeroU32::new(value)
            .map(Self)
            .ok_or(CombinedInputError::InvalidLimit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tagged::gc::mutator_registration) enum CombinedInputError {
    Authority(RegistrationError),
    InvalidLimit,
    EmptyInputs,
    Capacity,
    Role(CombinedInputRole),
    Function(FunctionError),
}

/// Identity/role metadata only. No Value, SharedRoot, definition or name word
/// is retained, so this receipt cannot mask a sole-root collector witness.
/// It describes the frozen inputs at admission, not future cell contents.
/// Every running operation must still validate its actual operand and key.
/// Immutability is a closed-tape restriction, not a property of SymbolHandle.
#[must_use = "bind these checked inputs to the closed mixed tape before executing"]
pub(in crate::tagged::gc::mutator_registration) struct AdmittedCombinedInputs {
    target: PreparedNameFunction,
    inputs: Box<[CombinedInput]>,
}
impl AdmittedCombinedInputs {
    pub(in crate::tagged::gc::mutator_registration) fn validate_running(
        &self,
        run: &RunAccess<'_, '_>,
    ) -> Result<(), RegistrationError> {
        self.target.validate_running(run)?;
        for input in &self.inputs {
            run.check_symbol(&input.key)?;
        }
        Ok(())
    }

    pub(in crate::tagged::gc::mutator_registration) fn inputs(&self) -> &[CombinedInput] {
        &self.inputs
    }

    pub(in crate::tagged::gc::mutator_registration) fn target(&self) -> PreparedNameFunction {
        self.target
    }

    /// A retained receipt cannot authorize another heap/era or stale backing.
    /// This rechecks identity, not the historical input values or their roles.
    pub(in crate::tagged::gc::mutator_registration) fn validate_identity(
        &self,
        keeper: &KeeperSymbols<'_>,
    ) -> Result<(), RegistrationError> {
        self.target.validate_keeper(keeper)?;
        for input in &self.inputs {
            keeper.backing(&input.key)?;
        }
        Ok(())
    }
}

fn check_role(
    value: Value,
    role: CombinedInputRole,
    lambda: SymId,
) -> Result<(), CombinedInputError> {
    match role {
        CombinedInputRole::PlainScalar => {
            if value.is_nil() || value.is_t() || value.is_fixnum() {
                Ok(())
            } else {
                Err(CombinedInputError::Role(role))
            }
        }
        CombinedInputRole::DirectName => {
            let name = value
                .as_lisp_string()
                .ok_or(CombinedInputError::Role(role))?;
            if name.has_intervals() {
                return Err(CombinedInputError::Role(role));
            }
            // This is a graph-sharing check. Keyword/spelling eligibility is
            // still decided by C5's intern operation, never copied here.
            Ok(())
        }
        CombinedInputRole::NameDefinition | CombinedInputRole::ScalarDefinition => {
            let subset = match role {
                CombinedInputRole::NameDefinition => functions::ReturnSubset::ScalarOrName,
                CombinedInputRole::ScalarDefinition => functions::ReturnSubset::Scalar,
                CombinedInputRole::PlainScalar | CombinedInputRole::DirectName => {
                    return Err(CombinedInputError::Role(role));
                }
            };
            let answer = functions::frozen_constant(value, lambda, subset)
                .map_err(CombinedInputError::Function)?;
            if role == CombinedInputRole::NameDefinition && answer.as_lisp_string().is_none() {
                return Err(CombinedInputError::Role(role));
            }
            Ok(())
        }
    }
}

impl KeeperSymbols<'_> {
    pub(in crate::tagged::gc::mutator_registration) fn admit_combined_inputs(
        &self,
        target: &PreparedNameFunction,
        inputs: &[CombinedInput],
        limit: CombinedInputLimit,
    ) -> Result<AdmittedCombinedInputs, CombinedInputError> {
        if inputs.is_empty() {
            return Err(CombinedInputError::EmptyInputs);
        }
        if inputs.len() > limit.0.get() as usize {
            return Err(CombinedInputError::Capacity);
        }
        let lambda = target
            .keeper_lambda(self)
            .map_err(CombinedInputError::Authority)?;
        // Validate ALL keys before dereferencing even the first input. This
        // covers heap/World/era/generation, poison, word zero and Plain backing.
        for input in inputs {
            self.backing(&input.key)
                .map_err(CombinedInputError::Authority)?;
        }
        // Lambda came from the existing matching prepared record, without
        // looking up the interner's serial TLS caches or any graph payload.
        for input in inputs {
            // SAFETY: every key belongs to this retained stopped heap/World;
            // the actual keeper still holds word zero, and backing() checked
            // Plain/projected-Plain before the test-only rooted value probe.
            let bits = unsafe { self.registry.symbols.backing.probe(input.key.symbol) };
            check_role(Value::from_bits(bits), input.role, lambda)?;
        }
        Ok(AdmittedCombinedInputs {
            target: *target,
            inputs: inputs.into(),
        })
    }
}

static_assertions::assert_impl_all!(CombinedInput: Send, Sync, Copy);
static_assertions::assert_impl_all!(AdmittedCombinedInputs: Send, Sync);
static_assertions::assert_not_impl_any!(AdmittedCombinedInputs: Clone, Copy);
static_assertions::assert_eq_size!(CombinedInputLimit, u32);
