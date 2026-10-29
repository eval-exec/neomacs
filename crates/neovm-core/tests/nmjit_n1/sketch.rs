//! N1 proposed API shapes only. This module is included by test crates, never
//! by product code. It grants no actual heap, registration or JIT authority.
#![allow(dead_code)]
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CollectorMode {
    Gen0,
    Gen1,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AdmissionError {
    GenerationalRequired,
}
pub(crate) struct ExclusiveCollector {
    mode: CollectorMode,
}
impl ExclusiveCollector {
    pub(crate) fn fixture(mode: CollectorMode) -> Self {
        Self { mode }
    }
    pub(crate) fn check_gen1(&mut self) -> Result<Gen1Admission<'_>, AdmissionError> {
        match self.mode {
            CollectorMode::Gen0 => Err(AdmissionError::GenerationalRequired),
            CollectorMode::Gen1 => Ok(Gen1Admission { collector: self }),
        }
    }
}
#[must_use]
pub(crate) struct Gen1Admission<'heap> {
    collector: &'heap mut ExclusiveCollector,
}
#[must_use]
pub(crate) struct ProductionEra<'heap> {
    admission: Gen1Admission<'heap>,
}
impl<'heap> ProductionEra<'heap> {
    pub(crate) fn from_admission(admission: Gen1Admission<'heap>) -> Self {
        Self { admission }
    }
    pub(crate) fn enter<'era>(&'era self, _code: ParallelCompiledCode) -> JitEntry<'era, 'heap> {
        JitEntry { era: self }
    }
    pub(crate) fn stopped(&mut self) -> StoppedEra<'_, 'heap> {
        StoppedEra { era: self }
    }
}
#[must_use]
pub(crate) struct JitEntry<'era, 'heap> {
    era: &'era ProductionEra<'heap>,
}
pub(crate) struct SerialCompiledCode {
    confined: PhantomData<Rc<()>>,
}
pub(crate) struct ParallelCompiledCode {
    confined: PhantomData<Rc<()>>,
}
#[must_use]
pub(crate) struct StoppedEra<'era, 'heap> {
    era: &'era mut ProductionEra<'heap>,
}
impl StoppedEra<'_, '_> {
    pub(crate) fn grace(&self) -> JitGracePeriod<'_> {
        JitGracePeriod {
            stopped: PhantomData,
        }
    }
}
#[must_use]
pub(crate) struct JitGracePeriod<'stop> {
    stopped: PhantomData<&'stop ()>,
}
pub(crate) struct RetiredTables;
impl RetiredTables {
    pub(crate) fn reclaim(&mut self, _grace: JitGracePeriod<'_>) {}
}
#[derive(Clone, Copy)]
pub(crate) struct ChunkSlot(pub(crate) usize);
#[must_use]
pub(crate) struct TableView<'view> {
    entries: &'view [ChunkSlot],
    confined: PhantomData<Rc<()>>,
}
impl<'view> TableView<'view> {
    pub(crate) fn fixture(entries: &'view [ChunkSlot]) -> Self {
        Self {
            entries,
            confined: PhantomData,
        }
    }
}

// Deliberately absent in the non-test contract client. No normal Cargo feature
// exposes a proof bypass; even test builds have no conversion or enter method.
#[cfg(test)]
pub(crate) struct ProofEra {
    confined: PhantomData<Rc<()>>,
}
#[cfg(test)]
impl ProofEra {
    pub(crate) fn fixture() -> Self {
        Self {
            confined: PhantomData,
        }
    }
}
