//! Control-state model. Mutex protection is intentional; visibility is a
//! separate experiment. Words/counters are abstract roots/logs, never VM data.
#![allow(dead_code)]

use std::marker::PhantomData;
use std::rc::Rc;

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::{Condvar, Mutex};
use loom::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub heap: usize,
    pub world: usize,
    pub slot: usize,
    pub generation: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Stopping(usize),
    Tracing(usize),
    DrainingResumes,
    Poisoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Running,
    Publishing,
    Stopped,
    NativeStopped,
    Resuming,
    StartingStopped,
    Retired,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fault {
    #[default]
    None,
    AckBeforeRoots,
    AckBeforeLogs,
    IgnoreLoan,
    StaleRegistration,
    StalePublication,
    StaleEpoch,
    NativeBeforeRoots,
    IgnoreResumeDebt,
    NativeReturnWithoutDebt,
    NativeReturnWithoutGate,
    RefoldNativeImage,
    DropRetiringLog,
    RemoveFrozenOwner,
    OmitDebtNotification,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Poisoned,
    Identity,
    Loan,
    Publication,
    Generation,
    Busy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Active(usize),
    Queued,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub key: Key,
    pub generation: u8,
    pub epoch: usize,
    pub root: usize,
    pub log_words: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: Key,
    pub mode: Mode,
    pub record: Option<Record>,
    pub publication_generation: u8,
    pub coverage_epoch: usize,
    pub run_loan: bool,
    pub worker_loan: bool,
    pub local_root: usize,
    pub pending: usize,
    pub issued: usize,
    pub folded: usize,
    pub image_epoch: usize,
    pub image_resident: bool,
    pub image_takes: usize,
    pub retiring: bool,
    pub restored: bool,
    pub bootstrap_root: bool,
}

impl Entry {
    fn new(slot: usize, generation: u8, mode: Mode) -> Self {
        Self {
            key: Key {
                heap: 11,
                world: 17,
                slot,
                generation,
            },
            mode,
            record: None,
            publication_generation: 0,
            coverage_epoch: 0,
            run_loan: false,
            worker_loan: false,
            local_root: 100 + slot,
            pending: 1,
            issued: 1,
            folded: 0,
            image_epoch: 0,
            image_resident: false,
            image_takes: 0,
            retiring: false,
            restored: true,
            // A not-yet-runnable registration's input is rooted by bootstrap.
            bootstrap_root: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub phase: Phase,
    pub epoch: usize,
    pub completed: usize,
    pub queued: bool,
    pub debt: usize,
    pub entries: Vec<Entry>,
    pub frozen: Vec<Key>,
    pub traces: usize,
    pub active_readers: usize,
}

pub struct Rendezvous {
    pub control: Mutex<State>,
    wake: Condvar,
    allocator: Mutex<usize>,
    request_word: AtomicUsize,
    completion_word: AtomicUsize,
    fault: Fault,
    generation_limit: u8,
}

impl Rendezvous {
    pub fn new(owners: usize) -> Self {
        Self::configured(owners, Fault::None, u8::MAX)
    }

    pub fn configured(owners: usize, fault: Fault, generation_limit: u8) -> Self {
        Self {
            control: Mutex::new(State {
                phase: Phase::Idle,
                epoch: 0,
                completed: 0,
                queued: false,
                debt: 0,
                entries: (0..owners)
                    .map(|i| Entry::new(i, 1, Mode::Running))
                    .collect(),
                frozen: Vec::new(),
                traces: 0,
                active_readers: 0,
            }),
            wake: Condvar::new(),
            allocator: Mutex::new(0),
            request_word: AtomicUsize::new(0),
            completion_word: AtomicUsize::new(0),
            fault,
            generation_limit,
        }
    }

    pub fn snapshot(&self) -> State {
        self.control.lock().unwrap().clone()
    }
    pub fn key(&self, slot: usize) -> Key {
        self.control.lock().unwrap().entries[slot].key
    }
    pub fn owner(&self, key: Key) -> Owner<'_> {
        Self::index(&self.control.lock().unwrap(), key).unwrap();
        Owner {
            runtime: self,
            key,
            _thread: PhantomData,
        }
    }

    fn index(s: &State, key: Key) -> Result<usize, Refusal> {
        let e = s.entries.get(key.slot).ok_or(Refusal::Identity)?;
        if e.key != key || e.mode == Mode::Retired {
            return Err(Refusal::Identity);
        }
        Ok(key.slot)
    }

    fn live(s: &State) -> Result<(), Refusal> {
        if s.phase == Phase::Poisoned {
            Err(Refusal::Poisoned)
        } else {
            Ok(())
        }
    }

    fn activate(&self, s: &mut State) -> usize {
        // Independent oracle: debt-free activation may not overlap restoration.
        assert!(
            s.entries.iter().all(|e| e.mode != Mode::Resuming),
            "RESTORATION_DURING_STOP"
        );
        assert!(s.debt == 0, "RESTORATION_DURING_STOP");
        s.epoch = s.epoch.checked_add(1).expect("request epoch exhausted");
        s.frozen = s
            .entries
            .iter()
            .filter(|e| !matches!(e.mode, Mode::Retired | Mode::StartingStopped))
            .map(|e| e.key)
            .collect();
        for e in &mut s.entries {
            if e.mode == Mode::NativeStopped {
                e.coverage_epoch = s.epoch;
            }
        }
        s.phase = Phase::Stopping(s.epoch);
        s.queued = false;
        self.request_word.store(s.epoch, Ordering::Release);
        self.wake.notify_all();
        s.epoch
    }

    pub fn request(&self) -> Result<Request, Refusal> {
        let mut s = self.control.lock().unwrap();
        Self::live(&s)?;
        if let Phase::Stopping(e) | Phase::Tracing(e) = s.phase {
            return Ok(Request::Active(e));
        }
        if (s.phase == Phase::DrainingResumes || s.debt != 0)
            && self.fault != Fault::IgnoreResumeDebt
        {
            s.queued = true;
            return Ok(Request::Queued);
        }
        Ok(Request::Active(self.activate(&mut s)))
    }

    fn publish(&self, s: &mut State, key: Key, native: bool, ready: bool) -> Result<(), Refusal> {
        Self::live(s)?;
        let i = Self::index(s, key)?;
        if s.entries[i].mode != Mode::Running
            || s.entries[i].image_resident
            || matches!(s.phase, Phase::Tracing(_))
        {
            return Err(Refusal::Busy);
        }
        if !native && !s.frozen.contains(&key) {
            return Err(Refusal::Busy);
        }
        if !ready {
            return Err(Refusal::Publication);
        }
        if (s.entries[i].run_loan || s.entries[i].worker_loan) && self.fault != Fault::IgnoreLoan {
            return Err(Refusal::Loan);
        }
        let generation = s.entries[i]
            .publication_generation
            .checked_add(1)
            .filter(|g| *g <= self.generation_limit)
            .ok_or(Refusal::Generation)?;
        let epoch = match s.phase {
            Phase::Stopping(e) => e,
            _ if native => s.completed,
            _ => return Err(Refusal::Busy),
        };
        let e = &mut s.entries[i];
        let mut record = Record {
            key,
            generation,
            epoch,
            root: e.local_root,
            log_words: e.pending,
        };
        if self.fault == Fault::StaleRegistration {
            record.key.generation = 0;
        }
        if self.fault == Fault::StalePublication {
            record.generation = 0;
        }
        if self.fault == Fault::StaleEpoch {
            record.epoch = epoch.saturating_sub(1);
        }
        if self.fault == Fault::AckBeforeRoots {
            record.root = 0;
        }
        e.publication_generation = generation;
        e.record = if self.fault == Fault::NativeBeforeRoots && native {
            None
        } else {
            Some(record)
        };
        if self.fault == Fault::AckBeforeLogs {
            e.pending = 0;
        }
        e.image_resident = true;
        e.restored = false;
        e.coverage_epoch = epoch;
        e.mode = if native {
            Mode::NativeStopped
        } else {
            Mode::Stopped
        };
        self.wake.notify_all();
        Ok(())
    }

    pub fn ack(&self, key: Key, ready: bool) -> Result<(), Refusal> {
        let mut s = self.control.lock().unwrap();
        self.publish(&mut s, key, false, ready)
    }

    pub fn enter_native(&self, key: Key) -> Result<(), Refusal> {
        let mut s = self.control.lock().unwrap();
        self.publish(&mut s, key, true, true)
    }

    pub fn retire(&self, key: Key) -> Result<(), Refusal> {
        let mut s = self.control.lock().unwrap();
        self.publish(&mut s, key, true, true)?;
        let e = &mut s.entries[key.slot];
        e.retiring = true;
        if self.fault == Fault::DropRetiringLog {
            e.pending = 0;
        }
        if self.fault == Fault::RemoveFrozenOwner {
            e.mode = Mode::Retired;
        }
        Ok(())
    }

    pub fn set_worker_loan(&self, key: Key, outstanding: bool) {
        let mut s = self.control.lock().unwrap();
        Self::index(&s, key).unwrap();
        if outstanding {
            assert!(
                s.entries[key.slot].mode == Mode::Running && !matches!(s.phase, Phase::Tracing(_)),
                "LOAN_WITHOUT_AUTHORITY"
            );
        }
        s.entries[key.slot].worker_loan = outstanding;
    }

    pub fn verify_stopped(&self, s: &State) {
        assert!(s.debt == 0, "RESTORATION_DURING_STOP");
        for key in &s.frozen {
            let e = &s.entries[key.slot];
            assert!(
                e.key == *key && e.mode != Mode::Retired,
                "FROZEN_OWNER_REMOVED"
            );
            assert!(!e.run_loan && !e.worker_loan, "OUTSTANDING_LOAN");
            let record = e.record.expect("ROOT_COVERAGE");
            assert!(record.key == *key, "STALE_REGISTRATION");
            assert!(
                record.generation == e.publication_generation,
                "STALE_PUBLICATION"
            );
            assert!(e.coverage_epoch == s.epoch, "STALE_ACK_EPOCH");
            assert!(
                record.epoch == s.epoch
                    || (e.mode == Mode::NativeStopped && record.epoch <= s.epoch),
                "STALE_ACK_EPOCH"
            );
            assert!(
                record.root == e.local_root && e.image_resident,
                "ROOT_COVERAGE"
            );
        }
        for e in s.entries.iter().filter(|e| e.mode == Mode::StartingStopped) {
            assert!(e.bootstrap_root, "STARTING_ROOT_LOST");
        }
    }

    pub fn try_stop(&self) -> Result<Option<AllStopped<'_>>, Refusal> {
        let mut s = self.control.lock().unwrap();
        Self::live(&s)?;
        if !matches!(s.phase, Phase::Stopping(_)) {
            return Ok(None);
        }
        // A removed participant is corruption, not an excuse to wait forever.
        assert!(
            s.frozen
                .iter()
                .all(|k| s.entries[k.slot].key == *k && s.entries[k.slot].mode != Mode::Retired),
            "FROZEN_OWNER_REMOVED"
        );
        if !s
            .frozen
            .iter()
            .all(|k| matches!(s.entries[k.slot].mode, Mode::Stopped | Mode::NativeStopped))
        {
            return Ok(None);
        }
        self.verify_stopped(&s);
        let epoch = s.epoch;
        s.phase = Phase::Tracing(epoch);
        Ok(Some(AllStopped {
            runtime: self,
            epoch,
            traced: false,
            finished: false,
            _thread: PhantomData,
        }))
    }

    pub fn stop(&self) -> Result<AllStopped<'_>, Refusal> {
        self.request()?;
        let mut s = self.control.lock().unwrap();
        loop {
            Self::live(&s)?;
            if s.phase == Phase::Idle {
                self.activate(&mut s);
            }
            if matches!(s.phase, Phase::Stopping(_)) {
                assert!(
                    s.frozen
                        .iter()
                        .all(|k| s.entries[k.slot].key == *k
                            && s.entries[k.slot].mode != Mode::Retired),
                    "FROZEN_OWNER_REMOVED"
                );
                if s.frozen
                    .iter()
                    .all(|k| matches!(s.entries[k.slot].mode, Mode::Stopped | Mode::NativeStopped))
                {
                    self.verify_stopped(&s);
                    let epoch = s.epoch;
                    s.phase = Phase::Tracing(epoch);
                    return Ok(AllStopped {
                        runtime: self,
                        epoch,
                        traced: false,
                        finished: false,
                        _thread: PhantomData,
                    });
                }
            }
            // The same mutex protects predicate and atomic unlock-and-wait.
            // Neither an early notify nor a spurious wake grants authority.
            s = self.wake.wait(s).unwrap();
        }
    }

    fn finish_debt(&self, s: &mut State) {
        if s.debt == 0 {
            s.phase = Phase::Idle;
            for e in &mut s.entries {
                if e.mode == Mode::StartingStopped {
                    e.mode = Mode::Running;
                }
            }
        }
        // One abstract resume-debt signaling edge. Production currently has
        // redundant notifications in finish_debt and its callers; this fault
        // omits the entire abstract edge, not just one redundant product line.
        if self.fault != Fault::OmitDebtNotification {
            self.wake.notify_all();
        }
    }

    pub fn begin_restore(&self, key: Key, native_return: bool) -> Result<usize, Refusal> {
        let mut s = self.control.lock().unwrap();
        loop {
            Self::live(&s)?;
            Self::index(&s, key)?;
            if !matches!(s.phase, Phase::Stopping(_) | Phase::Tracing(_))
                || (native_return && self.fault == Fault::NativeReturnWithoutGate)
            {
                break;
            }
            s = self.wake.wait(s).unwrap();
        }
        let e = &s.entries[key.slot];
        assert!(
            !matches!(s.phase, Phase::Stopping(_) | Phase::Tracing(_)),
            "NATIVE_RETURN_DURING_STOP"
        );
        assert!(e.image_resident && !e.retiring, "IMAGE_OWNERSHIP");
        assert!(
            if native_return {
                e.mode == Mode::NativeStopped
            } else {
                e.mode == Mode::Stopped
            },
            "IMAGE_OWNERSHIP"
        );
        let epoch = e.image_epoch;
        s.entries[key.slot].mode = Mode::Resuming;
        if self.fault != Fault::NativeReturnWithoutDebt || !native_return {
            s.debt |= 1 << key.slot;
            s.phase = Phase::DrainingResumes;
        }
        Ok(epoch)
    }

    pub fn finish_restore(&self, key: Key) -> Result<(), Refusal> {
        let mut s = self.control.lock().unwrap();
        Self::live(&s)?;
        Self::index(&s, key)?;
        assert!(s.phase != Phase::Tracing(s.epoch), "RUN_DURING_TRACE");
        assert!(s.debt & (1 << key.slot) != 0, "RESUME_DEBT");
        let e = &mut s.entries[key.slot];
        assert!(
            e.mode == Mode::Resuming && e.image_resident,
            "IMAGE_OWNERSHIP"
        );
        e.image_resident = false;
        e.image_takes += 1;
        e.restored = true;
        e.mode = Mode::Running;
        s.debt &= !(1 << key.slot);
        self.finish_debt(&mut s);
        Ok(())
    }

    pub fn wait_completed(&self, epoch: usize) -> Result<(), Refusal> {
        let mut s = self.control.lock().unwrap();
        Self::live(&s)?;
        while s.completed < epoch {
            Self::live(&s)?;
            s = self.wake.wait(s).unwrap();
        }
        assert!(
            self.completion_word.load(Ordering::Acquire) >= epoch,
            "COMPLETION_VISIBILITY"
        );
        Ok(())
    }

    pub fn register(&self) -> Result<Key, Refusal> {
        let mut s = self.control.lock().unwrap();
        Self::live(&s)?;
        let mode = if s.phase == Phase::Idle {
            Mode::Running
        } else {
            Mode::StartingStopped
        };
        let e = Entry::new(s.entries.len(), 1, mode);
        let key = e.key;
        s.entries.push(e);
        Ok(key)
    }

    pub fn finish_retire(&self, key: Key) -> Result<(), Refusal> {
        let mut s = self.control.lock().unwrap();
        Self::live(&s)?;
        Self::index(&s, key)?;
        if matches!(s.phase, Phase::Stopping(_) | Phase::Tracing(_)) || s.active_readers != 0 {
            return Err(Refusal::Busy);
        }
        let e = &mut s.entries[key.slot];
        assert!(
            e.retiring && e.pending == 0 && e.folded == e.issued,
            "RETIRED_LOG_LOST"
        );
        // The caller/result root is independently held before the old entry ends.
        assert!(e.bootstrap_root, "RETIRED_ROOT_LOST");
        e.mode = Mode::Retired;
        e.record = None;
        Ok(())
    }

    pub fn renew(&self, slot: usize) -> Result<Key, Refusal> {
        let mut s = self.control.lock().unwrap();
        Self::live(&s)?;
        if s.phase != Phase::Idle || s.entries[slot].mode != Mode::Retired {
            return Err(Refusal::Busy);
        }
        let generation = s.entries[slot]
            .key
            .generation
            .checked_add(1)
            .filter(|g| *g <= self.generation_limit)
            .ok_or(Refusal::Generation)?;
        let e = Entry::new(slot, generation, Mode::Running);
        let key = e.key;
        s.entries[slot] = e;
        Ok(key)
    }

    pub fn poison(&self) {
        let mut s = self.control.lock().unwrap();
        s.phase = Phase::Poisoned;
        self.wake.notify_all();
    }

    pub fn notify(&self) {
        self.wake.notify_all();
    }

    pub fn begin_reader(&self) {
        let mut s = self.control.lock().unwrap();
        assert!(
            matches!(s.phase, Phase::Tracing(_)),
            "READER_WITHOUT_AUTHORITY"
        );
        s.active_readers += 1;
    }

    pub fn finish_reader(&self) {
        let mut s = self.control.lock().unwrap();
        assert!(s.active_readers > 0);
        s.active_readers -= 1;
    }
}

pub struct Owner<'r> {
    runtime: &'r Rendezvous,
    key: Key,
    _thread: PhantomData<Rc<()>>,
}

impl Owner<'_> {
    pub fn run_access_after_resume_drain(&self) -> Result<RunAccess<'_>, Refusal> {
        self.run_access_observing_wait(|| {})
    }

    /// The observer forces a dangerous wait interleaving in a model history.
    /// It runs at most once under the predicate mutex, before atomic wait;
    /// production admission has no corresponding callback or field.
    pub fn run_access_observing_wait(
        &self,
        on_wait: impl FnOnce(),
    ) -> Result<RunAccess<'_>, Refusal> {
        let mut on_wait = Some(on_wait);
        let mut s = self.runtime.control.lock().unwrap();
        loop {
            Rendezvous::live(&s)?;
            let i = Rendezvous::index(&s, self.key)?;
            let e = &s.entries[i];
            // An owner must restore itself before waiting for other owners.
            // Otherwise its own debt could never be discharged while parked.
            if e.mode != Mode::Running || e.image_resident || !e.restored {
                return Err(Refusal::Busy);
            }
            if e.run_loan || e.worker_loan {
                return Err(Refusal::Loan);
            }
            match s.phase {
                Phase::Idle => {
                    assert_eq!(s.debt, 0, "RUN_BEFORE_RESUME_DRAIN");
                    s.entries[i].run_loan = true;
                    // Admission is selected while the gate remains held. The
                    // real API invokes unchanged run_access under this gate;
                    // the existing admission model proves its atomic pair.
                    drop(s);
                    return Ok(RunAccess {
                        runtime: self.runtime,
                        key: self.key,
                        _thread: PhantomData,
                    });
                }
                Phase::DrainingResumes => {
                    if let Some(observe) = on_wait.take() {
                        observe();
                    }
                    s = self.runtime.wake.wait(s).unwrap();
                }
                Phase::Stopping(_) | Phase::Tracing(_) => return Err(Refusal::Busy),
                Phase::Poisoned => return Err(Refusal::Poisoned),
            }
        }
    }

    pub fn allocate(&self) -> Result<(), Refusal> {
        {
            let mut s = self.runtime.control.lock().unwrap();
            Rendezvous::live(&s)?;
            let i = Rendezvous::index(&s, self.key)?;
            assert!(
                s.entries[i].mode == Mode::Running
                    && !s.entries[i].image_resident
                    && s.entries[i].restored,
                "RUN_WITHOUT_AUTHORITY"
            );
            assert!(!matches!(s.phase, Phase::Tracing(_)), "RUN_DURING_TRACE");
            assert!(!s.entries[i].run_loan, "DUPLICATE_RUN_LOAN");
            s.entries[i].run_loan = true;
        }
        {
            let mut grant = self.runtime.allocator.lock().unwrap();
            *grant += 1;
            thread::yield_now();
        }
        let mut s = self.runtime.control.lock().unwrap();
        let e = &mut s.entries[self.key.slot];
        e.local_root += 100;
        e.pending += 1;
        e.issued += 1;
        e.run_loan = false;
        self.runtime.wake.notify_all();
        Ok(())
    }

    pub fn wait_request(&self) -> Result<usize, Refusal> {
        let mut s = self.runtime.control.lock().unwrap();
        loop {
            Rendezvous::live(&s)?;
            Rendezvous::index(&s, self.key)?;
            if let Phase::Stopping(epoch) = s.phase {
                if s.frozen.contains(&self.key) {
                    assert!(
                        self.runtime.request_word.load(Ordering::Acquire) == epoch,
                        "REQUEST_VISIBILITY"
                    );
                    return Ok(epoch);
                }
            }
            s = self.runtime.wake.wait(s).unwrap();
        }
    }

    pub fn park_once(&self) -> Result<usize, Refusal> {
        let epoch = self.wait_request()?;
        self.runtime.ack(self.key, true)?;
        self.runtime.wait_completed(epoch)?;
        let image = self.runtime.begin_restore(self.key, false)?;
        assert!(image == epoch, "STALE_RESUME_IMAGE");
        thread::yield_now();
        self.runtime.finish_restore(self.key)?;
        Ok(epoch)
    }

    pub fn return_native(&self) -> Result<usize, Refusal> {
        let image = self.runtime.begin_restore(self.key, true)?;
        thread::yield_now();
        self.runtime.finish_restore(self.key)?;
        Ok(image)
    }
}

/// A model-only admitted run loan; no VM data or real RunAccess is included.
pub struct RunAccess<'r> {
    runtime: &'r Rendezvous,
    key: Key,
    _thread: PhantomData<Rc<()>>,
}

impl Drop for RunAccess<'_> {
    fn drop(&mut self) {
        let mut s = self.runtime.control.lock().unwrap();
        let e = &mut s.entries[self.key.slot];
        assert!(e.run_loan, "RUN_LOAN_LOST");
        e.run_loan = false;
    }
}

pub struct AllStopped<'r> {
    runtime: &'r Rendezvous,
    epoch: usize,
    traced: bool,
    finished: bool,
    _thread: PhantomData<Rc<()>>,
}

impl AllStopped<'_> {
    pub fn trace(&mut self) {
        let mut s = self.runtime.control.lock().unwrap();
        assert!(
            s.phase == Phase::Tracing(self.epoch) && !self.traced,
            "COLLECTOR_AUTHORITY"
        );
        self.runtime.verify_stopped(&s);
        assert!(
            self.runtime.allocator.try_lock().is_ok(),
            "ALLOCATOR_HELD_AT_TRACE"
        );
        let frozen = s.frozen.clone();
        for key in frozen {
            let e = &mut s.entries[key.slot];
            let pending = if self.runtime.fault == Fault::RefoldNativeImage
                && e.mode == Mode::NativeStopped
            {
                e.record.unwrap().log_words
            } else {
                e.pending
            };
            e.folded += pending;
            e.pending = 0;
            assert!(e.folded <= e.issued, "DUPLICATE_LOG_FOLD");
            assert!(e.folded == e.issued, "LOG_COVERAGE");
            e.image_epoch = self.epoch;
        }
        s.traces += 1;
        self.traced = true;
    }

    pub fn resume(mut self) {
        let mut s = self.runtime.control.lock().unwrap();
        assert!(
            s.phase == Phase::Tracing(self.epoch) && self.traced,
            "COLLECTOR_AUTHORITY"
        );
        assert!(s.active_readers == 0, "READER_STILL_ACTIVE");
        s.completed = self.epoch;
        for key in s.frozen.clone() {
            if s.entries[key.slot].mode == Mode::Stopped {
                s.debt |= 1 << key.slot;
            }
        }
        s.phase = Phase::DrainingResumes;
        self.runtime
            .completion_word
            .store(self.epoch, Ordering::Release);
        self.runtime.finish_debt(&mut s);
        self.finished = true;
    }
}

impl Drop for AllStopped<'_> {
    fn drop(&mut self) {
        // A failing oracle must not be replaced by a mutex-poison double panic.
        // Normal unfinished exit is a terminal poison, never implicit resume.
        if !self.finished && !std::thread::panicking() {
            self.runtime.poison();
        }
    }
}

static_assertions::assert_not_impl_any!(Owner<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RunAccess<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(AllStopped<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(Key: Send, Sync, Copy);
