use std::sync::mpsc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use neomacs_display_protocol::VideoId;

const QUERY_COUNT: u32 = 2;
const QUERY_BUFFER_BYTES: u64 = QUERY_COUNT as u64 * size_of::<u64>() as u64;
const TIMER_POOL_SIZE: usize = 8;
const GPU_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const GPU_WAIT_QUANTUM: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MeasurementEpoch(u64);

impl MeasurementEpoch {
    fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    fn accepts(self, sample: &CompletedGpuFrameTiming) -> bool {
        sample.epoch == self
    }
}

struct TimerSlot {
    query_set: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    readback: wgpu::Buffer,
}

pub(crate) struct PendingGpuFrameTiming {
    slot: TimerSlot,
    epoch: MeasurementEpoch,
}

impl PendingGpuFrameTiming {
    pub(crate) fn timestamp_writes(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.slot.query_set,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: Some(1),
        }
    }

    pub(crate) fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.resolve_query_set(&self.slot.query_set, 0..QUERY_COUNT, &self.slot.resolved, 0);
        encoder.copy_buffer_to_buffer(
            &self.slot.resolved,
            0,
            &self.slot.readback,
            0,
            QUERY_BUFFER_BYTES,
        );
    }
}

pub(crate) struct CompletedGpuFrameTiming {
    pub(crate) video_ids: Vec<VideoId>,
    pub(crate) duration_us: u64,
    epoch: MeasurementEpoch,
}

struct RetiredTimerSlot {
    slot: TimerSlot,
    sample: Option<CompletedGpuFrameTiming>,
}

enum WorkerCommand {
    Read {
        submission: wgpu::SubmissionIndex,
        slot: TimerSlot,
        video_ids: Vec<VideoId>,
        epoch: MeasurementEpoch,
    },
    Shutdown,
}

/// A bounded, asynchronous pool for GPU timestamp queries.
///
/// The render thread never waits for a timestamp.  If all slots are still in
/// flight, that frame is simply not sampled.  One FIFO worker waits for queue
/// completion, maps the tiny result buffer, and returns the slot for reuse.
pub(crate) struct GpuFrameTimer {
    status: neomacs_video::VideoGpuTimingStatus,
    available: Vec<TimerSlot>,
    worker_tx: Option<mpsc::Sender<WorkerCommand>>,
    completed_rx: mpsc::Receiver<RetiredTimerSlot>,
    worker: Option<JoinHandle<()>>,
    epoch: MeasurementEpoch,
    cancelled: Arc<AtomicBool>,
}

impl GpuFrameTimer {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self::with_requested(
            device,
            queue,
            std::env::var_os("NEOMACS_GPU_FRAME_TIMING").as_deref()
                == Some(std::ffi::OsStr::new("1")),
        )
    }

    fn with_requested(device: &wgpu::Device, queue: &wgpu::Queue, requested: bool) -> Self {
        let (completed_tx, completed_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        if !requested {
            return Self {
                status: neomacs_video::VideoGpuTimingStatus::Disabled,
                available: Vec::new(),
                worker_tx: None,
                completed_rx,
                worker: None,
                epoch: MeasurementEpoch::default(),
                cancelled,
            };
        }
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return Self {
                status: neomacs_video::VideoGpuTimingStatus::Unsupported,
                available: Vec::new(),
                worker_tx: None,
                completed_rx,
                worker: None,
                epoch: MeasurementEpoch::default(),
                cancelled,
            };
        }

        let available = (0..TIMER_POOL_SIZE)
            .map(|index| TimerSlot {
                query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some(&format!("Video frame timer queries {index}")),
                    ty: wgpu::QueryType::Timestamp,
                    count: QUERY_COUNT,
                }),
                // WebGPU deliberately keeps query resolution and host mapping
                // in separate usages. Resolve on-GPU first, then copy the
                // sixteen bytes into the reusable staging buffer below.
                resolved: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("Video frame timer resolved timestamps {index}")),
                    size: QUERY_BUFFER_BYTES,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("Video frame timer readback {index}")),
                    size: QUERY_BUFFER_BYTES,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
            })
            .collect();
        let timestamp_period_ns = queue.get_timestamp_period();
        let device = device.clone();
        let (worker_tx, worker_rx) = mpsc::channel();
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("neomacs-gpu-frame-timing".to_owned())
            .spawn(move || {
                while let Some(command) = receive_until_cancelled(&worker_rx, &worker_cancelled) {
                    let WorkerCommand::Read {
                        submission,
                        slot,
                        video_ids,
                        epoch,
                    } = command
                    else {
                        break;
                    };
                    let sample = read_timestamp_sample(
                        &device,
                        submission,
                        &slot.readback,
                        timestamp_period_ns,
                        &worker_cancelled,
                    )
                    .map(|duration_us| CompletedGpuFrameTiming {
                        video_ids,
                        duration_us,
                        epoch,
                    });
                    if completed_tx
                        .send(RetiredTimerSlot { slot, sample })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("spawn GPU frame timing worker");

        Self {
            status: neomacs_video::VideoGpuTimingStatus::Enabled,
            available,
            worker_tx: Some(worker_tx),
            completed_rx,
            worker: Some(worker),
            epoch: MeasurementEpoch::default(),
            cancelled,
        }
    }

    pub(crate) const fn status(&self) -> neomacs_video::VideoGpuTimingStatus {
        self.status
    }

    pub(crate) fn begin(&mut self) -> Option<PendingGpuFrameTiming> {
        self.available.pop().map(|slot| PendingGpuFrameTiming {
            slot,
            epoch: self.epoch,
        })
    }

    pub(crate) fn submit(
        &mut self,
        pending: PendingGpuFrameTiming,
        submission: wgpu::SubmissionIndex,
        video_ids: Vec<VideoId>,
    ) {
        let Some(worker_tx) = &self.worker_tx else {
            self.available.push(pending.slot);
            return;
        };
        let command = WorkerCommand::Read {
            submission,
            slot: pending.slot,
            video_ids,
            epoch: pending.epoch,
        };
        if let Err(error) = worker_tx.send(command) {
            if let WorkerCommand::Read { slot, .. } = error.0 {
                self.available.push(slot);
            }
        }
    }

    pub(crate) fn drain(&mut self) -> Vec<CompletedGpuFrameTiming> {
        let mut samples = Vec::new();
        let epoch = self.epoch;
        self.reclaim_completed(|sample| {
            if epoch.accepts(&sample) {
                samples.push(sample);
            }
        });
        samples
    }

    pub(crate) fn begin_measurement_epoch(&mut self) {
        self.epoch = self.epoch.next();
    }

    fn reclaim_completed(&mut self, mut observe: impl FnMut(CompletedGpuFrameTiming)) {
        while let Ok(retired) = self.completed_rx.try_recv() {
            if let Some(sample) = retired.sample {
                observe(sample);
            }
            self.available.push(retired.slot);
        }
    }
}

impl Drop for GpuFrameTimer {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(worker_tx) = self.worker_tx.take() {
            let _ = worker_tx.send(WorkerCommand::Shutdown);
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("GPU frame timing worker panicked");
        }
    }
}

fn read_timestamp_sample(
    device: &wgpu::Device,
    submission: wgpu::SubmissionIndex,
    buffer: &wgpu::Buffer,
    timestamp_period_ns: f32,
    cancelled: &AtomicBool,
) -> Option<u64> {
    let (mapped_tx, mapped_rx) = mpsc::sync_channel(1);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = mapped_tx.send(result);
    });
    // IO TIMEOUT, NOT A VISUAL PHASE: this bounds a blocking wait for the GPU
    // to finish a submission, on a worker thread with no frame in hand. A
    // frame-constant sample would make the remaining time never shrink and the
    // loop never time out.
    let deadline = Instant::now() + GPU_WAIT_TIMEOUT;
    let submitted = wait_until(deadline, cancelled, |quantum| {
        match device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission.clone()),
            timeout: Some(quantum),
        }) {
            Ok(_) => WaitProgress::Ready(()),
            Err(wgpu::PollError::Timeout) => WaitProgress::Pending,
            Err(wgpu::PollError::WrongSubmissionIndex(_, _)) => WaitProgress::Failed,
        }
    });
    let mapped = submitted.and_then(|()| {
        wait_until(deadline, cancelled, |quantum| {
            match mapped_rx.recv_timeout(quantum) {
                Ok(Ok(())) => WaitProgress::Ready(()),
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => WaitProgress::Failed,
                Err(mpsc::RecvTimeoutError::Timeout) => WaitProgress::Pending,
            }
        })
    });
    if mapped.is_none() {
        buffer.unmap();
        return None;
    }
    let Ok(mapped) = slice.get_mapped_range() else {
        buffer.unmap();
        return None;
    };
    let timestamps: &[u64] = bytemuck::cast_slice(&mapped);
    let duration_us = timestamp_delta_us(timestamps[0], timestamps[1], timestamp_period_ns);
    drop(mapped);
    buffer.unmap();
    duration_us
}

enum WaitProgress<T> {
    Ready(T),
    Pending,
    Failed,
}

fn wait_until<T>(
    deadline: Instant,
    cancelled: &AtomicBool,
    mut wait_once: impl FnMut(Duration) -> WaitProgress<T>,
) -> Option<T> {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return None;
        }
        // Real clock read on purpose: the whole point is that this shrinks
        // between iterations. See `read_timestamp_sample`.
        let remaining = deadline.checked_duration_since(Instant::now())?;
        let quantum = remaining.min(GPU_WAIT_QUANTUM);
        match wait_once(quantum) {
            WaitProgress::Ready(value) => return Some(value),
            WaitProgress::Pending => {}
            WaitProgress::Failed => return None,
        }
    }
}

fn receive_until_cancelled<T>(receiver: &mpsc::Receiver<T>, cancelled: &AtomicBool) -> Option<T> {
    let value = receiver.recv().ok()?;
    (!cancelled.load(Ordering::Acquire)).then_some(value)
}

fn timestamp_delta_us(start: u64, end: u64, timestamp_period_ns: f32) -> Option<u64> {
    let ticks = end.checked_sub(start)?;
    let duration_us = ticks as f64 * f64::from(timestamp_period_ns) / 1_000.0;
    duration_us.is_finite().then(|| duration_us.round() as u64)
}

#[cfg(test)]
#[path = "gpu_frame_timing_test.rs"]
mod tests;
