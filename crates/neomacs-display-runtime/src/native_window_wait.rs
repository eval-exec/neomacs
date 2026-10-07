//! Exact lease for synchronous native window construction. The controller may
//! interrupt only the retained connection's duplicated socket, never native objects.
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

#[derive(Default)]
struct State {
    leases: HashMap<u64, (Arc<AtomicBool>, Instant)>,
    active: Option<u64>,
    terminal: bool,
}
#[derive(Default)]
pub struct NativeWindowWaits(Mutex<State>);
impl NativeWindowWaits {
    pub fn register(&self, frame: u64, live: Arc<AtomicBool>, deadline: Instant) {
        self.0
            .lock()
            .unwrap()
            .leases
            .insert(frame, (live, deadline));
    }
    pub fn remove(&self, frame: u64) {
        self.0.lock().unwrap().leases.remove(&frame);
    }
    pub fn terminal(&self) -> bool {
        self.0.lock().unwrap().terminal
    }
    /// Claim terminal connection disposition before the exact socket shutdown.
    /// Finishing the synchronous call is serialized with this claim.
    pub fn interrupt_required(&self) -> bool {
        let mut state = self.0.lock().unwrap();
        if state
            .active
            .and_then(|frame| state.leases.get(&frame))
            .is_some_and(|(live, deadline)| {
                !live.load(Ordering::Acquire) || Instant::now() >= *deadline
            })
        {
            state.terminal = true;
        }
        state.terminal
    }
    pub fn run<T>(
        &self,
        frame: u64,
        work: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        {
            let mut state = self.0.lock().unwrap();
            if state.terminal
                || state.leases.get(&frame).is_some_and(|(live, deadline)| {
                    !live.load(Ordering::Acquire) || Instant::now() >= *deadline
                })
            {
                return Err("Native frame preparation cancelled".into());
            }
            state.active = Some(frame);
        }
        let result = work();
        let mut state = self.0.lock().unwrap();
        state.active = None;
        if state.terminal
            || state.leases.get(&frame).is_some_and(|(live, deadline)| {
                !live.load(Ordering::Acquire) || Instant::now() >= *deadline
            })
        {
            return Err("Native frame preparation cancelled".into());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_revocation_terminalizes_only_its_connection() {
        let waits = Arc::new(NativeWindowWaits::default());
        let live = Arc::new(AtomicBool::new(true));
        waits.register(
            1,
            live.clone(),
            Instant::now() + std::time::Duration::from_secs(15),
        );
        let result = waits.run(1, || {
            live.store(false, Ordering::Release);
            assert!(waits.interrupt_required());
            Ok(42)
        });
        assert!(result.is_err());
        assert!(waits.terminal());
        let fresh = Arc::new(AtomicBool::new(true));
        waits.register(
            2,
            fresh,
            Instant::now() + std::time::Duration::from_secs(15),
        );
        assert!(waits.run(2, || Ok(42)).is_err());
    }
    #[test]
    fn completed_constructor_loses_interrupt_authority() {
        let waits = NativeWindowWaits::default();
        let live = Arc::new(AtomicBool::new(true));
        waits.register(
            1,
            live.clone(),
            Instant::now() + std::time::Duration::from_secs(15),
        );
        assert_eq!(waits.run(1, || Ok(42)), Ok(42));
        live.store(false, Ordering::Release);
        assert!(!waits.interrupt_required());
        assert!(!waits.terminal());
    }
}
