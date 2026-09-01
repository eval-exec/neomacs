use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::bootstrap::{build_render_event_loop_any_thread, run_render_loop_with_event_loop};
use super::{SharedImageRenderState, SharedMonitorInfo};
use crate::thread_comm::RenderComms;

/// Render thread state.
pub struct RenderThread {
    handle: Option<JoinHandle<()>>,
}

impl RenderThread {
    fn finish_spawn(
        handle: JoinHandle<()>,
        startup_rx: mpsc::Receiver<Result<(), String>>,
        startup_timeout: Duration,
    ) -> Result<Self, String> {
        match startup_rx.recv_timeout(startup_timeout) {
            Ok(Ok(())) => Ok(Self {
                handle: Some(handle),
            }),
            Ok(Err(err)) => {
                let _ = handle.join();
                Err(err)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "Timed out waiting {:?} for render thread startup",
                startup_timeout
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = handle.join();
                Err("Render thread terminated before reporting startup status".to_string())
            }
        }
    }

    #[allow(dead_code)] // spawn helpers exercised by the thread_handle tests
    fn spawn_with_state_timeout<T, S, R>(
        startup: S,
        runner: R,
        startup_timeout: Duration,
    ) -> Result<Self, String>
    where
        T: Send + 'static,
        S: FnOnce() -> Result<T, String> + Send + 'static,
        R: FnOnce(T) -> Result<(), String> + Send + 'static,
    {
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || match startup() {
            Ok(state) => {
                let _ = startup_tx.send(Ok(()));
                if let Err(err) = runner(state) {
                    tracing::error!("Render thread exited with error: {}", err);
                }
            }
            Err(err) => {
                let _ = startup_tx.send(Err(err));
            }
        });

        Self::finish_spawn(handle, startup_rx, startup_timeout)
    }

    #[allow(dead_code)] // exercised by the thread_handle tests
    fn spawn_with_state<T, S, R>(startup: S, runner: R) -> Result<Self, String>
    where
        T: Send + 'static,
        S: FnOnce() -> Result<T, String> + Send + 'static,
        R: FnOnce(T) -> Result<(), String> + Send + 'static,
    {
        Self::spawn_with_state_timeout(startup, runner, Duration::from_secs(10))
    }

    /// Spawn the render thread.
    pub fn spawn(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_metadata: SharedImageRenderState,
        shared_monitors: SharedMonitorInfo,
    ) -> Result<Self, String> {
        #[cfg(feature = "neo-term")]
        let shared_terminals = crate::terminal::new_shared_terminals();
        Self::spawn_inner(
            comms,
            width,
            height,
            title,
            image_metadata,
            shared_monitors,
            #[cfg(feature = "neo-term")]
            shared_terminals,
        )
    }

    #[cfg(feature = "neo-term")]
    pub fn spawn_with_terminals(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_metadata: SharedImageRenderState,
        shared_monitors: SharedMonitorInfo,
        shared_terminals: crate::terminal::SharedTerminals,
    ) -> Result<Self, String> {
        Self::spawn_inner(
            comms,
            width,
            height,
            title,
            image_metadata,
            shared_monitors,
            shared_terminals,
        )
    }

    fn spawn_inner(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_metadata: SharedImageRenderState,
        shared_monitors: SharedMonitorInfo,
        #[cfg(feature = "neo-term")] shared_terminals: crate::terminal::SharedTerminals,
    ) -> Result<Self, String> {
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || match build_render_event_loop_any_thread() {
            Ok(event_loop) => {
                let _ = startup_tx.send(Ok(()));
                if let Err(err) = run_render_loop_with_event_loop(
                    event_loop,
                    comms,
                    width,
                    height,
                    title,
                    image_metadata,
                    shared_monitors,
                    true,
                    #[cfg(feature = "neo-term")]
                    shared_terminals,
                ) {
                    tracing::error!("Render thread exited with error: {}", err);
                }
            }
            Err(err) => {
                let _ = startup_tx.send(Err(err));
            }
        });

        Self::finish_spawn(handle, startup_rx, Duration::from_secs(10))
    }

    /// Wait for render thread to finish.
    pub fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
#[path = "thread_handle_test.rs"]
mod tests;
