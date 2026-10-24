//! Initial-obarray name operations: operands and answers remain owner roots.
use super::*;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::value::TaggedValue as Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialInternError {
    Authority(RegistrationError),
    ExpectedString,
    UnsupportedName,
}
impl From<RegistrationError> for InitialInternError {
    fn from(error: RegistrationError) -> Self {
        Self::Authority(error)
    }
}

/// An era/World-bound obarray reader, without a pre-existing symbol key.
/// It borrows RunAccess until release and shares the C2/C3 authority word.
pub(super) struct InitialObarrayRef<'lease, 'owner, 'era> {
    pub(super) run: &'lease mut RunAccess<'owner, 'era>,
}
impl Drop for InitialObarrayRef<'_, '_, '_> {
    fn drop(&mut self) {
        word::reader_exit(&self.run.owner.registry.symbols.word);
        self.run.symbol_loan = false;
    }
}

impl<'owner, 'era> RunAccess<'owner, 'era> {
    fn initial_obarray_ref(
        &mut self,
    ) -> Result<InitialObarrayRef<'_, 'owner, 'era>, RegistrationError> {
        self.owner.check_thread()?;
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let symbols = &self.owner.registry.symbols;
        if symbols.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if !word::reader_enter(&symbols.word) {
            return Err(RegistrationError::Busy);
        }
        if symbols.poisoned.load(Ordering::Acquire) {
            word::reader_exit(&symbols.word);
            return Err(RegistrationError::Poisoned);
        }
        self.symbol_loan = true;
        Ok(InitialObarrayRef { run: self })
    }

    /// Ordinary initial-obarray intern-soft. NIL is a successful miss;
    /// contention is a typed refusal, never a miss. Both outcomes leave every
    /// registry/World storage and semantic clock unchanged. String contents
    /// are immutable for this closed era; no shorthand or callback is run.
    pub fn initial_intern_soft_into(
        &mut self,
        name: &OwnerRootSlot<'era>,
        dst: &mut OwnerRootSlot<'era>,
    ) -> Result<(), InitialInternError> {
        globals::check_home(self, name)?;
        globals::check_home(self, dst)?;
        self.initial_obarray_ref()?.soft_into(name.index, dst.index)
    }
}

impl InitialObarrayRef<'_, '_, '_> {
    fn soft_into(&mut self, name_index: usize, dst_index: usize) -> Result<(), InitialInternError> {
        let answer = {
            let local = self
                .run
                .owner
                .local
                .as_ref()
                .ok_or(RegistrationError::StaleRegistration)?;
            let value = local
                .root_scratch
                .get(name_index)
                .ok_or(RegistrationError::StaleRegistration)?
                .value();
            let name = value
                .as_lisp_string()
                .ok_or(InitialInternError::ExpectedString)?;
            // Keyword creation and keyword-tagged answers are outside this
            // proof's ordinary-name set, including its soft-lookup operator.
            if name.as_bytes().first() == Some(&b':') {
                return Err(InitialInternError::UnsupportedName);
            }
            let authority = self.read_token();
            match crate::emacs_core::intern::parallel::lookup(name, &authority)? {
                None => Value::NIL,
                Some(id) => {
                    // SAFETY: this lease borrows the exact era/World backing
                    // and holds its counted-reader word. No manager escapes.
                    let member = unsafe {
                        self.run
                            .owner
                            .registry
                            .symbols
                            .backing
                            .initial_contains(id, &authority)
                    }
                    .ok_or(RegistrationError::Publication)?;
                    if member {
                        Value::from_sym_id(id)
                    } else {
                        Value::NIL
                    }
                }
            }
        };
        // Source was copied before this store, so an aliased operand works.
        // No allocation/callback/poll occurs between selection and rooting,
        // and the reader remains held through installation into the real home.
        let dst = self
            .run
            .owner
            .local
            .as_mut()
            .and_then(|local| local.root_scratch.get_mut(dst_index))
            .ok_or(RegistrationError::StaleRegistration)?;
        *dst = MarkWord::of(answer);
        Ok(())
    }
}
static_assertions::assert_not_impl_any!(InitialObarrayRef<'static, 'static, 'static>: Send, Sync, Clone, Copy);
