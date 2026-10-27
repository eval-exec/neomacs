//! CPU image decoding has one owner, independent of any GPU generation.
//!
//! Clients submit generation-qualified jobs and observe validated metadata. Only
//! the owner drains pixel outcomes, so attaching a renderer never decodes twice.

use super::*;
use neomacs_display_protocol::ImageSizeLimit;
use std::collections::VecDeque;
use std::sync::Condvar;
use std::time::{Duration, Instant};

/// The CPU decoder and its not-yet-uploaded pixels. Move this owner across GPU
/// recreation rather than replacing the worker pool or decoding requests again.
pub struct ImageDecoder {
    next_id: AtomicU32,
    pub(super) client: ImageDecodeClient,
}

/// Submission and terminal CPU metadata observation, available before GPU setup.
#[derive(Clone)]
pub struct ImageDecodeClient {
    decode_tx: mpsc::Sender<DecodeRequest>,
    shared: Arc<DecodeShared>,
    pub(super) sequence_cache: Arc<ImageSequenceCache>,
}

#[derive(Default)]
pub(super) struct DecodeShared {
    state: Mutex<DecodeState>,
    changed: Condvar,
}

struct DecodeState {
    entries: HashMap<ImageId, DecodeEntry>,
    outcomes: VecDeque<WorkerDecodeOutcome>,
    owner_alive: bool,
    retired_sequences: HashSet<ImageSequenceId>,
    retired_before_submission: HashSet<ImageId>,
    retired_through: Option<ImageSequenceId>,
}

impl Default for DecodeState {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            outcomes: VecDeque::new(),
            owner_alive: true,
            retired_sequences: HashSet::new(),
            retired_before_submission: HashSet::new(),
            retired_through: None,
        }
    }
}

impl Drop for ImageDecoder {
    fn drop(&mut self) {
        let mut state = self
            .client
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.owner_alive = false;
        state.outcomes.clear();
    }
}

struct DecodeEntry {
    load: ImageLoadToken,
    sequence: Option<ImageSequenceId>,
    status: DecodeStatus,
}

enum DecodeStatus {
    Pending,
    Complete(Result<ImageMetadata, ImageDiagnostic>),
    Retired,
}

impl DecodeState {
    fn is_current(&self, load: ImageLoadToken) -> bool {
        self.entries.get(&load.image()).is_some_and(|entry| {
            entry.load == load && !matches!(entry.status, DecodeStatus::Retired)
        })
    }

    fn sequence_retired(&self, sequence: ImageSequenceId) -> bool {
        self.retired_sequences.contains(&sequence)
            || self.retired_through.is_some_and(|last| sequence <= last)
    }
}

pub(super) enum DecodeOutputSink {
    Shared(Arc<DecodeShared>),
    #[cfg(test)]
    Channel(mpsc::Sender<WorkerDecodeOutcome>),
}

impl DecodeOutputSink {
    pub(super) fn accepts(&self, load: ImageLoadToken) -> bool {
        match self {
            Self::Shared(shared) => shared
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_current(load),
            #[cfg(test)]
            Self::Channel(_) => true,
        }
    }

    pub(super) fn publish(&self, outcome: WorkerDecodeOutcome) {
        match self {
            Self::Shared(shared) => {
                let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
                let load = outcome.load();
                if !state.is_current(load) {
                    return;
                }
                let terminal = match &outcome {
                    WorkerDecodeOutcome::Band { .. } => None,
                    WorkerDecodeOutcome::Ready(decoded) => Some(Ok(decoded.metadata.clone())),
                    WorkerDecodeOutcome::Failed { diagnostic, .. } => Some(Err(diagnostic.clone())),
                };
                if let Some(terminal) = terminal {
                    state
                        .entries
                        .get_mut(&load.image())
                        .expect("current load")
                        .status = DecodeStatus::Complete(terminal);
                    shared.changed.notify_all();
                }
                if state.owner_alive {
                    state.outcomes.push_back(outcome);
                }
            }
            #[cfg(test)]
            Self::Channel(tx) => {
                let _ = tx.send(outcome);
            }
        }
    }
}

impl Default for ImageDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageDecoder {
    #[must_use]
    pub fn new() -> Self {
        let (decode_tx, decode_rx) = mpsc::channel();
        let shared = Arc::new(DecodeShared::default());
        let sequence_cache = Arc::new(ImageSequenceCache::new());
        let decode_rx = Arc::new(Mutex::new(decode_rx));
        let pool_size = ImageDecoderPoolSize::detected();
        for thread_id in 0..pool_size.get() {
            let rx = Arc::clone(&decode_rx);
            let sink = DecodeOutputSink::Shared(Arc::clone(&shared));
            let sequences = Arc::clone(&sequence_cache);
            thread::spawn(move || {
                ImageCache::decoder_thread_pooled(thread_id, rx, sink, sequences)
            });
        }
        Self {
            next_id: AtomicU32::new(1),
            client: ImageDecodeClient {
                decode_tx,
                shared,
                sequence_cache,
            },
        }
    }

    pub(super) fn allocate_image_id(&self) -> ImageId {
        let raw = self
            .next_id
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .expect("image identity space exhausted");
        ImageId::new(raw)
    }

    #[must_use]
    pub fn client(&self) -> ImageDecodeClient {
        self.client.clone()
    }

    pub(super) fn take_pending(&mut self, loads: &ImageLoadLifecycle) -> Vec<WorkerDecodeOutcome> {
        let mut state = self
            .client
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut ready = Vec::new();
        let mut waiting = VecDeque::new();
        while let Some(outcome) = state.outcomes.pop_front() {
            if !state.is_current(outcome.load()) {
                continue;
            }
            if loads.is_current(outcome.load()) {
                ready.push(outcome);
            } else if !loads.was_admitted(outcome.load()) {
                // The CPU job may finish before its renderer command arrives.
                waiting.push_back(outcome);
            }
        }
        state.outcomes = waiting;
        ready
    }
}

impl ImageDecodeClient {
    pub(super) fn accepts(&self, load: ImageLoadToken) -> bool {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        !state.retired_before_submission.contains(&load.image())
            && state.entries.get(&load.image()).is_none_or(|entry| {
                load.attempt() > entry.load.attempt()
                    || (entry.load == load && !matches!(entry.status, DecodeStatus::Retired))
            })
    }

    pub(super) fn submit(&self, request: DecodeRequest) {
        let load = request.load;
        let sequence = match &request.source {
            ImageSource::File { sequence, .. } | ImageSource::Data { sequence, .. } => {
                Some(*sequence)
            }
            _ => None,
        };
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            // Retain the completed token too: later GPU adoption must not decode
            // it again, and delayed older requests must never replace a new one.
            if state.retired_before_submission.contains(&load.image())
                || state
                    .entries
                    .get(&load.image())
                    .is_some_and(|entry| load.attempt() <= entry.load.attempt())
                || sequence.is_some_and(|sequence| state.sequence_retired(sequence))
            {
                return;
            }
            if state.entries.contains_key(&load.image()) {
                state
                    .outcomes
                    .retain(|outcome| outcome.load().image() != load.image());
            }
            state.entries.insert(
                load.image(),
                DecodeEntry {
                    load,
                    sequence,
                    status: DecodeStatus::Pending,
                },
            );
            self.shared.changed.notify_all();
        }
        if self.decode_tx.send(request).is_err() {
            DecodeOutputSink::Shared(Arc::clone(&self.shared)).publish(
                WorkerDecodeOutcome::Failed {
                    load,
                    diagnostic: ImageDiagnostic::NotDrawable,
                },
            );
        }
    }

    /// Submit a file decode. Repeating a token is a no-op, even after completion.
    pub fn load_file_with_id(
        &self,
        load: ImageLoadToken,
        path: &str,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        animation: ImageAnimationPolicy,
        frame: ImageFrameIndex,
        sequence: ImageSequenceId,
        identity: ImageLoadIdentity,
        limit: ImageSizeLimit,
    ) {
        self.submit(DecodeRequest {
            load,
            source: ImageSource::File {
                path: path.to_owned(),
                sequence,
            },
            size,
            rotation,
            realization,
            colors,
            mask,
            animation,
            frame,
            identity,
            limit,
        });
    }

    /// Submit encoded data for full decoding, not merely a header measurement.
    pub fn load_data_with_id(
        &self,
        load: ImageLoadToken,
        data: EncodedBytes,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        animation: ImageAnimationPolicy,
        frame: ImageFrameIndex,
        sequence: ImageSequenceId,
        resources: crate::SvgResourceContext,
        identity: ImageLoadIdentity,
        limit: ImageSizeLimit,
    ) {
        self.submit(DecodeRequest {
            load,
            source: ImageSource::Data {
                data,
                resources,
                sequence,
            },
            size,
            rotation,
            realization,
            colors,
            mask,
            animation,
            frame,
            identity,
            limit,
        });
    }

    /// Wait for this exact generation's fully decoded metadata. Retirement or
    /// supersession terminates the wait; neither can publish an older result.
    #[must_use]
    pub fn wait_for_terminal(
        &self,
        load: ImageLoadToken,
        timeout: Duration,
    ) -> Option<Result<ImageMetadata, ImageDiagnostic>> {
        let started = Instant::now();
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if state.retired_before_submission.contains(&load.image()) {
                return None;
            }
            if let Some(entry) = state.entries.get(&load.image()) {
                if entry.load.attempt() > load.attempt() {
                    return None;
                }
                if entry.load == load {
                    match &entry.status {
                        DecodeStatus::Complete(result) => return Some(result.clone()),
                        DecodeStatus::Retired => return None,
                        DecodeStatus::Pending => {}
                    }
                }
                // An older entry can remain while this newer request is still
                // resolving its source off-thread. Wait for its admission.
            }
            let remaining = timeout.checked_sub(started.elapsed())?;
            let (next, _) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|e| e.into_inner());
            state = next;
        }
    }

    /// Invalidate CPU outcomes immediately, including pixels awaiting a GPU.
    pub fn retire_image(&self, image: ImageId) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = state.entries.get_mut(&image) {
            entry.status = DecodeStatus::Retired;
        } else {
            // A deferred path resolver may not have submitted its job yet.
            // Catalog invalidation allocates a fresh ID: this never-admitted
            // identity is retired permanently, including its delayed request.
            state.retired_before_submission.insert(image);
        }
        state
            .outcomes
            .retain(|outcome| outcome.load().image() != image);
        self.shared.changed.notify_all();
    }

    pub fn retire_sequence(&self, retirement: ImageSequenceRetirement) {
        self.sequence_cache.retire(retirement);
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        match retirement {
            ImageSequenceRetirement::One(sequence) => {
                state.retired_sequences.insert(sequence);
            }
            ImageSequenceRetirement::AllocatedThrough(last) => {
                state.retired_through =
                    Some(state.retired_through.map_or(last, |old| old.max(last)));
                state.retired_sequences.retain(|sequence| *sequence > last);
            }
        }
        let retired: HashSet<_> = state
            .entries
            .iter()
            .filter_map(|(image, entry)| {
                entry
                    .sequence
                    .filter(|sequence| state.sequence_retired(*sequence))
                    .map(|_| *image)
            })
            .collect();
        for image in &retired {
            state.entries.get_mut(image).expect("existing entry").status = DecodeStatus::Retired;
        }
        state
            .outcomes
            .retain(|outcome| !retired.contains(&outcome.load().image()));
        self.shared.changed.notify_all();
    }

    pub(super) fn clear(&self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        for entry in state.entries.values_mut() {
            entry.status = DecodeStatus::Retired;
        }
        state.outcomes.clear();
        self.shared.changed.notify_all();
    }
}

#[cfg(test)]
#[path = "tests/image_decoder_test.rs"]
mod tests;
