//! Scalar diagnostic selection, independent of backend admission policy.
//! Configuration is process-wide; selected classes carry no mutator/heap state.
//! Only read after the optional diagnostic path is enabled.

use super::{CompileOrigin, JobClass};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiagnosticClass {
    Job(JobClass),
    Synchronous,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ClassSelection {
    #[default]
    EntryOnly,
    All,
}

impl ClassSelection {
    pub(super) fn parse(value: Option<&str>) -> Self {
        match value {
            Some("all") => Self::All,
            _ => Self::EntryOnly,
        }
    }

    pub(super) fn select(
        self,
        origin: CompileOrigin,
        class: Option<JobClass>,
    ) -> Option<DiagnosticClass> {
        match self {
            Self::EntryOnly => class
                .filter(|c| *c == JobClass::Entry)
                .map(DiagnosticClass::Job),
            Self::All => Some(class.map_or_else(
                || match origin {
                    CompileOrigin::Dispatch | CompileOrigin::DeferralExpired => {
                        DiagnosticClass::Job(JobClass::Entry)
                    }
                    CompileOrigin::FirstSight => DiagnosticClass::Job(JobClass::FirstSight),
                    CompileOrigin::Osr => DiagnosticClass::Job(JobClass::Osr),
                    CompileOrigin::Retier | CompileOrigin::AotDrain | CompileOrigin::Direct => {
                        DiagnosticClass::Synchronous
                    }
                },
                DiagnosticClass::Job,
            )),
        }
    }
}

pub(super) fn selection() -> ClassSelection {
    static SELECTION: OnceLock<ClassSelection> = OnceLock::new();
    *SELECTION.get_or_init(|| {
        ClassSelection::parse(
            std::env::var("NEOVM_JIT_FRONT_DIAGNOSTIC_CLASSES")
                .ok()
                .as_deref(),
        )
    })
}
