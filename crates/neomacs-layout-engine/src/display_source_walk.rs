use crate::display_source::DisplaySourceTextPosition;
use crate::display_source_progress::DisplaySourceProgressState;

pub(crate) struct DisplaySourcePositionConsumption<T> {
    value: T,
    source_position: DisplaySourceTextPosition,
}

impl<T> DisplaySourcePositionConsumption<T> {
    pub(crate) fn new(value: T, source_position: DisplaySourceTextPosition) -> Self {
        Self {
            value,
            source_position,
        }
    }

    pub(crate) fn apply_to_progress(self, progress: &mut DisplaySourceProgressState<'_>) -> T {
        progress.apply_source_position(self.source_position);
        self.value
    }
}
