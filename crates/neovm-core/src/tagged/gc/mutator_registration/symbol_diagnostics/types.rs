//! Diagnostic domains and the test-tool boundary. No product setting.
use super::*;

/// Reader rows require a bound global. The shared-VOID initial state is
/// useful for intern publication tests, but is not a diagnostic row choice.
#[derive(Clone, Copy, Debug)]
pub(super) enum DiagnosticBacking {
    Plain,
    ProjectedPlain,
}
impl DiagnosticBacking {
    pub(super) fn initial(self) -> PlainSymbolBacking {
        match self {
            Self::Plain => PlainSymbolBacking::Unprojected,
            Self::ProjectedPlain => PlainSymbolBacking::Projected,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Owners {
    One,
    Two,
    Four,
}
impl Owners {
    pub(super) fn count(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Workload {
    Readers,
    PeriodicWriter,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum DiagnosticMode {
    Gate,
    Measurement,
}
impl DiagnosticMode {
    pub(super) fn load() -> Self {
        match std::env::var("P3SYM_C6_DIAGNOSTIC") {
            Err(std::env::VarError::NotPresent) => Self::Gate,
            Ok(value) if value == "measure" => Self::Measurement,
            Ok(value) => panic!("invalid test-tool diagnostic mode: {value:?}"),
            Err(std::env::VarError::NotUnicode(value)) => {
                panic!("invalid test-tool diagnostic mode: {value:?}")
            }
        }
    }
    pub(super) fn reads(self) -> Reads {
        match self {
            Self::Gate => Reads(128),
            Self::Measurement => Reads(1_000_000),
        }
    }
    pub(super) fn stride(self) -> Reads {
        match self {
            Self::Gate => Reads(8),
            Self::Measurement => Reads(4096),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ordinal(pub(super) usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Reads(pub(super) usize);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Busy(pub(super) u64);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Checksum(pub(super) usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Peers(pub(super) u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Collections(pub(super) usize);
#[derive(Debug, PartialEq, Eq)]
pub(super) struct WriterSample {
    pub(super) wait: Duration,
    pub(super) active_peers: Peers,
    pub(super) busy: Busy,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReadRecord {
    pub(super) owner: Ordinal,
    pub(super) started: Instant,
    pub(super) ended: Instant,
    pub(super) reads: Reads,
    pub(super) checksum: Checksum,
    pub(super) run_busy: Busy,
    pub(super) read_busy: Busy,
    pub(super) writers: Vec<WriterSample>,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Registration(RegistrationError),
    Global(GlobalError),
    Rendezvous(RendezvousError),
    Channel,
    Deadline,
    Read(Box<ReadFailure>),
    Affinity,
    Aborted,
}
impl From<RegistrationError> for Error {
    fn from(error: RegistrationError) -> Self {
        Self::Registration(error)
    }
}
impl From<GlobalError> for Error {
    fn from(error: GlobalError) -> Self {
        Self::Global(error)
    }
}
impl From<RendezvousError> for Error {
    fn from(error: RendezvousError) -> Self {
        Self::Rendezvous(error)
    }
}

#[derive(Default)]
pub(super) struct ActiveOwners(pub(super) AtomicUsize);

#[derive(Debug)]
pub(super) struct WaitExpired;

enum Pending {
    Idle,
    Waiting(Deadline),
}
/// Mint a clock only on refusal. A successful operation does not read it.
pub(super) struct Contention(Pending);
impl Contention {
    pub(super) fn new() -> Self {
        Self(Pending::Idle)
    }
    pub(super) fn refused(&mut self) -> Result<(), WaitExpired> {
        if let Pending::Idle = self.0 {
            self.0 = Pending::Waiting(Deadline::contention());
        }
        match &self.0 {
            Pending::Idle => unreachable!("deadline initialized above"),
            Pending::Waiting(deadline) => deadline.check(WaitExpired)?,
        }
        std::thread::yield_now();
        Ok(())
    }
    pub(super) fn completed(&mut self) {
        self.0 = Pending::Idle;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum WriterRefusal {
    Registration(RegistrationError),
    Deadline,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct WriterFailure {
    pub(super) sample: WriterSample,
    pub(super) refusal: WriterRefusal,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadRefusal {
    Run(RegistrationError),
    Global(GlobalError),
    RunDeadline,
    ReadDeadline,
    Writer(WriterFailure),
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReadFailure {
    pub(super) partial: ReadRecord,
    pub(super) refusal: ReadRefusal,
}
impl ReadRecord {
    pub(super) fn failed(mut self, refusal: ReadRefusal) -> Error {
        self.ended = Instant::now();
        Error::Read(Box::new(ReadFailure {
            partial: self,
            refusal,
        }))
    }
}
