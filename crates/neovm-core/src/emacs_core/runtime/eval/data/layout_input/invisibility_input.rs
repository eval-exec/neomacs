//! Layout input records for invisibility input (stage-4 W2/E5).

use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutInvisibilityInput(pub(crate) Selection);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Selection {
    #[default]
    None,
    All,
    Entries(Arc<[Entry]>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    // GNU first compares the property to the whole entry, including conses.
    pub(crate) identity: usize,
    pub(crate) category: Option<(usize, bool)>,
}
