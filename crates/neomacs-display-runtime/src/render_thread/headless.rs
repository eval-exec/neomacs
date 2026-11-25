//! A `RenderApp` without an event loop, a window or a GPU, for tests that
//! drive the render half of a `ThreadComms` from another crate
//! (DECISIONS D44, ruling (a)). MILESTONES M11b extends this entry.

use super::{RenderApp, SharedImageRenderState};
use crate::thread_comm::{InputEvent, RenderComms};
use neomacs_display_protocol::input_latency::InputToken;
use neomacs_display_protocol::input_progress::InputDelivery;
use std::sync::{Arc, Condvar, Mutex};

impl RenderApp {
    /// A render app over `comms`, the render half of a `ThreadComms`, whose
    /// primary window is pending at `width` x `height`. Nothing runs until
    /// the caller pumps [`Self::process_commands`]; it never polls when idle
    /// and reports no monitors.
    pub fn headless(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_metadata: SharedImageRenderState,
    ) -> Self {
        Self::new(
            comms,
            width,
            height,
            title,
            image_metadata,
            Arc::new((Mutex::new(Vec::new()), Condvar::new())),
            false,
            #[cfg(feature = "neo-term")]
            crate::terminal::new_shared_terminals(),
        )
    }

    /// Publish `event` to the evaluator as the platform input path does,
    /// with the receipt that path issues.
    pub fn send_input_with_receipt(
        &self,
        event: InputEvent,
    ) -> (Option<InputDelivery>, Option<InputToken>) {
        self.comms.send_input_with_receipt(event)
    }
}
