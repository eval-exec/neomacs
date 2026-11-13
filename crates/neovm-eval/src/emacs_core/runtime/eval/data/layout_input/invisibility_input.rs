//! Layout input records for invisibility input (stage-4 W2/E5).

use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutInvisibilityInput(pub Selection);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[doc(hidden)]
pub enum Selection {
    #[default]
    None,
    All,
    Entries(Arc<[Entry]>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct Entry {
    // GNU first compares the property to the whole entry, including conses.
    pub identity: usize,
    pub category: Option<(usize, bool)>,
}
