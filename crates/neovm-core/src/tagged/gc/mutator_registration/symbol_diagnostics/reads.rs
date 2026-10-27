//! The actual sealed read plus checked canonical root-home consumer.
use super::*;

fn writer(
    run: &mut RunAccess<'_, '_>,
    symbol: &SymbolHandle,
    ordinal: Ordinal,
    active: &ActiveOwners,
) -> Result<WriterSample, WriterFailure> {
    let started = Instant::now();
    let peers = Peers((active.0.load(Ordering::Acquire) & !(1 << ordinal.0)).count_ones());
    let mut busy = Busy::default();
    let deadline = Deadline::contention();
    loop {
        match run.symbol_mut(symbol) {
            Ok(lease) => {
                drop(lease);
                return Ok(WriterSample {
                    wait: started.elapsed(),
                    active_peers: peers,
                    busy,
                });
            }
            Err(error) => match contention::registration(&error) {
                Retry::Busy => {
                    busy.0 += 1;
                    if deadline.check(()).is_err() {
                        return Err(WriterFailure {
                            sample: WriterSample {
                                wait: started.elapsed(),
                                active_peers: peers,
                                busy,
                            },
                            refusal: WriterRefusal::Deadline,
                        });
                    }
                    thread::yield_now();
                }
                Retry::Refuse => {
                    return Err(WriterFailure {
                        sample: WriterSample {
                            wait: started.elapsed(),
                            active_peers: peers,
                            busy,
                        },
                        refusal: WriterRefusal::Registration(error),
                    });
                }
            },
        }
    }
}
struct Active<'a> {
    ordinal: Ordinal,
    bitmap: &'a ActiveOwners,
}
impl<'a> Active<'a> {
    fn enter(ordinal: Ordinal, bitmap: &'a ActiveOwners) -> Self {
        bitmap.0.fetch_or(1 << ordinal.0, Ordering::AcqRel);
        Self { ordinal, bitmap }
    }
}
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.bitmap
            .0
            .fetch_and(!(1 << self.ordinal.0), Ordering::AcqRel);
    }
}
pub(super) fn run<'era>(
    owner: &mut Mutator<'era>,
    home: &mut OwnerRootSlot<'era>,
    symbol: &SymbolHandle,
    ordinal: Ordinal,
    owners: Owners,
    workload: Workload,
    mode: DiagnosticMode,
    active: &ActiveOwners,
) -> Result<ReadRecord, Error> {
    let quota = Reads(mode.reads().0 / owners.count());
    let stride = mode.stride();
    // Reserve outside timing, so sample-vector growth cannot dominate the
    // separate writer-admission/read loop envelope.
    let sample_capacity = match (workload, ordinal) {
        (Workload::Readers, _) => 0,
        (Workload::PeriodicWriter, Ordinal(0)) => quota.0 / stride.0,
        (Workload::PeriodicWriter, _) => 0,
    };
    let writers = Vec::with_capacity(sample_capacity);
    let started = Instant::now();
    let _active = Active::enter(ordinal, active);
    let mut record = ReadRecord {
        owner: ordinal,
        started,
        ended: started,
        reads: Reads(0),
        checksum: Checksum(0),
        run_busy: Busy(0),
        read_busy: Busy(0),
        writers,
    };
    let run_deadline = Deadline::contention();
    loop {
        match owner.run_access() {
            Ok(mut access) => {
                let mut pending = Contention::new();
                while record.reads.0 < quota.0 {
                    match access.sym_value_into(symbol, home) {
                        Ok(()) => {
                            pending.completed();
                            let word = std::hint::black_box(home.word_for_test(&access));
                            record.checksum.0 = record.checksum.0.wrapping_add(word);
                            record.reads.0 += 1;
                        }
                        Err(error) => match contention::global(&error) {
                            Retry::Busy => {
                                record.read_busy.0 += 1;
                                if let Err(WaitExpired) = pending.refused() {
                                    return Err(record.failed(ReadRefusal::ReadDeadline));
                                }
                                continue;
                            }
                            Retry::Refuse => return Err(record.failed(ReadRefusal::Global(error))),
                        },
                    }
                    match workload {
                        Workload::Readers => {}
                        Workload::PeriodicWriter => {
                            if ordinal == Ordinal(0) && record.reads.0 % stride.0 == 0 {
                                match writer(&mut access, symbol, ordinal, active) {
                                    Ok(sample) => record.writers.push(sample),
                                    Err(failure) => {
                                        return Err(record.failed(ReadRefusal::Writer(failure)));
                                    }
                                }
                            }
                        }
                    }
                }
                record.ended = Instant::now();
                return Ok(record);
            }
            Err(error) => match contention::registration(&error) {
                Retry::Busy => {
                    record.run_busy.0 += 1;
                    if run_deadline.check(()).is_err() {
                        return Err(record.failed(ReadRefusal::RunDeadline));
                    }
                    thread::yield_now();
                }
                Retry::Refuse => return Err(record.failed(ReadRefusal::Run(error))),
            },
        }
    }
}
