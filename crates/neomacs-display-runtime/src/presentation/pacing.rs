//! Keep obsolete frames out of the interactive presentation queue.
//! The frame coordinator still paces work; mailbox replaces pending images
//! instead of making the newest input wait behind them. FIFO is universal.
pub(crate) const MAXIMUM_FRAME_LATENCY: u32 = 1;

pub(crate) fn present_mode(supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    if supported.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else {
        wgpu::PresentMode::Fifo
    }
}

#[cfg(test)]
#[path = "tests/pacing_test.rs"]
mod tests;
