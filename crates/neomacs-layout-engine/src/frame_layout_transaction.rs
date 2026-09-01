//! Bounded convergence for one speculative frame-layout transaction.

use crate::window_layout::WindowChromeMetrics;
use neomacs_display_protocol::types::DisplayWindowId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FrameRelayoutRequest {
    FrameTabBar {
        assumed_height: f32,
        measured_height: f32,
    },
    WindowChrome {
        window_id: DisplayWindowId,
        assumed: WindowChromeMetrics,
        measured: WindowChromeMetrics,
    },
    Minibuffer {
        window_id: DisplayWindowId,
        allocated_rows: usize,
        required_rows: usize,
    },
    /// Lisp entered during leaf-local fontification changed canonical layout
    /// inputs. Discard the speculative frame and recollect before replaying.
    LogicalInputsChanged {
        window_id: DisplayWindowId,
    },
    WindowTopologyChanged {
        before: u64,
        after: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutConvergenceError {
    pub(crate) retry_count: usize,
    pub(crate) max_retries: usize,
    pub(crate) last_request: FrameRelayoutRequest,
}

pub(crate) struct FrameLayoutCoordinator {
    retry_count: usize,
    max_retries: usize,
}

impl FrameLayoutCoordinator {
    pub(crate) fn new(max_retries: usize) -> Self {
        Self {
            retry_count: 0,
            max_retries,
        }
    }

    pub(crate) fn request_retry(
        &mut self,
        request: FrameRelayoutRequest,
    ) -> Result<(), LayoutConvergenceError> {
        if self.retry_count >= self.max_retries {
            return Err(LayoutConvergenceError {
                retry_count: self.retry_count,
                max_retries: self.max_retries,
                last_request: request,
            });
        }
        self.retry_count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_rejects_request_beyond_shared_retry_budget() {
        let mut coordinator = FrameLayoutCoordinator::new(1);
        let first = FrameRelayoutRequest::FrameTabBar {
            assumed_height: 16.0,
            measured_height: 24.0,
        };
        let second = FrameRelayoutRequest::Minibuffer {
            window_id: DisplayWindowId::new(7),
            allocated_rows: 1,
            required_rows: 3,
        };

        assert_eq!(coordinator.request_retry(first), Ok(()));
        assert_eq!(
            coordinator.request_retry(second),
            Err(LayoutConvergenceError {
                retry_count: 1,
                max_retries: 1,
                last_request: second,
            })
        );
    }
}
