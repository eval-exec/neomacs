//! Protocol-specific error classification, sharing the production-capable deadline.
pub(in super::super) use super::super::retry::{Deadline, Retry, SPIN_WAIT, retry, retry_owned};
use super::super::{
    FunctionError, GlobalError, InitialInternError, RegistrationError, RendezvousError,
};

pub(in super::super) fn registration(error: &RegistrationError) -> Retry {
    match error {
        RegistrationError::Busy => Retry::Busy,
        RegistrationError::WrongHeap
        | RegistrationError::WrongOwner
        | RegistrationError::StaleRegistration
        | RegistrationError::DuplicateOwner
        | RegistrationError::WrongThread
        | RegistrationError::Poisoned
        | RegistrationError::Exhausted
        | RegistrationError::Capacity
        | RegistrationError::OutstandingLoan
        | RegistrationError::OpenRegion
        | RegistrationError::ResidualSatb
        | RegistrationError::Publication
        | RegistrationError::PendingRoots
        | RegistrationError::PendingYoung => Retry::Refuse,
    }
}

pub(in super::super) fn initial_intern(error: &InitialInternError) -> Retry {
    match error {
        InitialInternError::Authority(error) => registration(error),
        InitialInternError::ExpectedString
        | InitialInternError::ExpectedPlainSymbol
        | InitialInternError::UnsupportedName => Retry::Refuse,
    }
}

pub(in super::super) fn global(error: &GlobalError) -> Retry {
    match error {
        GlobalError::Authority(error) => registration(error),
        GlobalError::VoidVariable => Retry::Refuse,
    }
}

pub(in super::super) fn function(error: &FunctionError) -> Retry {
    match error {
        FunctionError::Authority(error) => registration(error),
        FunctionError::VoidFunction
        | FunctionError::UnsupportedDefinition
        | FunctionError::EpochExhausted
        | FunctionError::ActiveActivation
        | FunctionError::InactiveActivation => Retry::Refuse,
    }
}

pub(in super::super) fn rendezvous(error: &RendezvousError) -> Retry {
    match error {
        RendezvousError::Busy => Retry::Busy,
        RendezvousError::Poisoned
        | RendezvousError::Identity
        | RendezvousError::Loan
        | RendezvousError::Publication
        | RendezvousError::Generation
        | RendezvousError::Readers
        | RendezvousError::Capacity
        | RendezvousError::Root
        | RendezvousError::Registration(_) => Retry::Refuse,
    }
}
