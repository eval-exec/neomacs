//! Frame-owned binding marks and checked bytecode unbind targets.

use smallvec::SmallVec;

/// Outstanding specpdl marks owned by one bytecode frame.
///
/// Each entry is the depth saved before that frame pushes a binding or unwind
/// action. Interpreter calls suspend this whole owner; OSR lends its marks and
/// transfers them back on resume. A bytecode count may consume only these marks,
/// never a caller's prefix or a prologue entry outside this mirror.
#[repr(transparent)]
#[derive(Default, Debug)]
pub(super) struct BindStack(SmallVec<[usize; 8]>);

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum FrameUnbindError {
    #[error("cannot unbind {requested} entries from a frame with {available} outstanding bindings")]
    TooMany { requested: usize, available: usize },
    #[error("frame binding mark {depth} is not below specpdl depth {specpdl_len}")]
    StaleMark { depth: usize, specpdl_len: usize },
}

/// A saved mark checked while consuming this frame's outstanding bindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FrameUnbindTarget(usize);

impl FrameUnbindTarget {
    pub(super) fn depth(self) -> usize {
        self.0
    }
}

impl BindStack {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn push(&mut self, depth: usize) {
        self.0.push(depth);
    }

    pub(super) fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }

    #[cfg(any(test, feature = "jit"))]
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    #[cfg(any(test, feature = "jit"))]
    pub(super) fn as_slice(&self) -> &[usize] {
        self.0.as_slice()
    }

    #[cfg(feature = "jit")]
    pub(super) fn extend_from_slice(&mut self, marks: &[usize]) {
        self.0.extend_from_slice(marks);
    }

    #[cfg(test)]
    pub(super) fn as_ptr(&self) -> *const usize {
        self.0.as_ptr()
    }

    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Validate before changing the mirror or unwinding any actual bindings.
    /// Bunbind0 is a no-op; a positive count must select one of this frame's
    /// saved pre-push marks still below the current specpdl top. No depth is
    /// guessed from the count or from the current native/OSR entry floor.
    pub(super) fn consume(
        &mut self,
        count: usize,
        specpdl_len: usize,
    ) -> Result<Option<FrameUnbindTarget>, FrameUnbindError> {
        if count == 0 {
            return Ok(None);
        }
        let keep = self
            .len()
            .checked_sub(count)
            .ok_or(FrameUnbindError::TooMany {
                requested: count,
                available: self.len(),
            })?;
        let depth = self.0[keep];
        if depth >= specpdl_len {
            return Err(FrameUnbindError::StaleMark { depth, specpdl_len });
        }
        self.truncate(keep);
        Ok(Some(FrameUnbindTarget(depth)))
    }
}

impl FromIterator<usize> for BindStack {
    fn from_iter<T: IntoIterator<Item = usize>>(marks: T) -> Self {
        Self(marks.into_iter().collect())
    }
}

#[cfg(test)]
impl Extend<usize> for BindStack {
    fn extend<T: IntoIterator<Item = usize>>(&mut self, marks: T) {
        self.0.extend(marks);
    }
}
